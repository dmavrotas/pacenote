//! Heuristic prompt linting.
//!
//! The goal is fewer wasted turns, not prettier prompts: a prompt with no
//! concrete target makes Claude spend tool calls (and tokens) searching for
//! it. Rules only fire on prompts that look like task requests, so
//! conversational replies ("yes", "go ahead") are never flagged.
//!
//! Supported languages: English, Greek, French, Spanish, German.

use serde::Serialize;

use crate::lang::{self, Lang};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
}

/// Pastes above this many estimated tokens get flagged.
const LARGE_PASTE_TOKENS: usize = 5_000;
/// Longer prompts usually carry enough description even without an anchor.
const VAGUE_MAX_WORDS: usize = 30;
const DONE_CRITERIA_MIN_WORDS: usize = 6;

/// Lint a prompt in any supported language (see [`crate::lang`]). Messages
/// are returned in the prompt's language.
pub fn lint(prompt: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let trimmed = prompt.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('!') {
        return findings; // slash commands and shell escapes
    }

    let words = words(trimmed);
    let packs = lang::detect(&words, trimmed);
    let packs = packs.as_slice();
    let lang = packs[0];
    let phrase = format!(" {} ", words.join(" "));
    let any = |list: fn(&Lang) -> &'static [&'static str]| {
        packs
            .iter()
            .any(|l| lang::mentions(list(l), &words, &phrase))
    };

    let est_tokens = trimmed.len() / 4;
    if est_tokens > LARGE_PASTE_TOKENS {
        findings.push(Finding {
            code: "large-paste",
            severity: Severity::Warn,
            message: lang
                .messages
                .large_paste
                .replace("{n}", &est_tokens.to_string()),
        });
    }

    let is_task = any(|l| l.task_verbs) || any(|l| l.task_phrases);
    if !is_task || is_question(trimmed, &words, packs) {
        return findings;
    }

    if words.len() <= VAGUE_MAX_WORDS
        && !has_anchor(trimmed, packs)
        && !any(|l| l.continuations)
        && words.iter().all(|w| is_generic(w, packs))
    {
        findings.push(Finding {
            code: "vague-target",
            severity: Severity::Warn,
            message: lang.messages.vague_target.to_string(),
        });
    }

    if words.len() >= DONE_CRITERIA_MIN_WORDS && !any(|l| l.done_markers) {
        findings.push(Finding {
            code: "no-done-criteria",
            severity: Severity::Info,
            message: lang.messages.no_done_criteria.to_string(),
        });
    }
    findings
}

/// The main language of a prompt (for localized messages).
pub fn detect_language(prompt: &str) -> &'static Lang {
    let trimmed = prompt.trim();
    lang::detect(&words(trimmed), trimmed)[0]
}

/// A single question ("why did you change that?") rather than a request.
/// Greek uses `;` as its question mark; Spanish opens questions with `¿`.
fn is_question(text: &str, words: &[String], packs: &[&Lang]) -> bool {
    let marks = text.matches(['?', ';', '\u{037e}']).count();
    text.ends_with(['?', ';', '\u{037e}'])
        && marks == 1
        && words
            .first()
            .is_some_and(|w| packs.iter().any(|l| lang::list_has(l.question_words, w)))
}

/// Is this word generic (or a task verb or symptom) in any active pack,
/// allowing simple English inflections?
fn is_generic(word: &str, packs: &[&Lang]) -> bool {
    let known = |w: &str| {
        packs.iter().any(|l| {
            lang::list_has(l.generic, w)
                || lang::list_has(l.task_verbs, w)
                // A symptom ("crashes", "σκάει") signals the task, not its target.
                || l.task_phrases
                    .iter()
                    .any(|p| !p.contains(' ') && lang::entry_matches(p, w))
        })
    };
    known(word)
        || ["s", "es", "ed", "d", "ing", "ly"]
            .iter()
            .filter_map(|suffix| word.strip_suffix(suffix))
            .any(known)
}

/// Folded words. French elisions are split off (`l'appli` → `l`, `appli`)
/// so the noun is checked on its own; English contractions stay whole.
fn words(text: &str) -> Vec<String> {
    lang::fold(text)
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .flat_map(|w| match w.split_once('\'') {
            Some((head, tail))
                if !tail.is_empty()
                    && matches!(head, "l" | "d" | "j" | "n" | "s" | "c" | "m" | "t" | "qu") =>
            {
                vec![head.to_string(), tail.to_string()]
            }
            _ => vec![w.trim_matches('\'').to_string()],
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Does the prompt point at something concrete: code, a path, an identifier,
/// a line number, or a quoted error?
fn has_anchor(text: &str, packs: &[&Lang]) -> bool {
    if text.contains('`') {
        return true;
    }
    if quoted_spans(text).any(|q| q.chars().count() > 3) {
        return true;
    }
    let tokens: Vec<&str> = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| ",;:!?¿¡()[]{}'\"«»„“”".contains(c)))
        .map(|t| t.trim_end_matches('.'))
        .collect();
    tokens.iter().any(|t| is_identifier_like(t))
        || tokens.windows(2).any(|w| {
            w[1].parse::<u32>().is_ok()
                && packs
                    .iter()
                    .any(|l| lang::list_has(l.line_words, &lang::fold(w[0])))
        })
}

/// Text inside "…", «…», „…“ or “…”.
fn quoted_spans(text: &str) -> impl Iterator<Item = &str> {
    [('"', '"'), ('«', '»'), ('„', '“'), ('“', '”')]
        .into_iter()
        .flat_map(move |(open, close)| {
            let mut spans = Vec::new();
            let mut rest = text;
            while let Some(start) = rest.find(open) {
                let after = &rest[start + open.len_utf8()..];
                let Some(end) = after.find(close) else { break };
                spans.push(&after[..end]);
                rest = &after[end + close.len_utf8()..];
            }
            spans
        })
}

fn is_identifier_like(token: &str) -> bool {
    if token.contains('/') || token.contains("::") || token.ends_with("()") {
        return true;
    }
    if token.contains('_') && token.chars().any(char::is_alphabetic) {
        return true;
    }
    // file.ext
    if let Some((stem, ext)) = token.rsplit_once('.') {
        if !stem.is_empty()
            && (1..=5).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return true;
        }
    }
    // camelCase / PascalCase with an inner capital
    token.chars().next().is_some_and(char::is_alphabetic)
        && token
            .chars()
            .zip(token.chars().skip(1))
            .any(|(a, b)| a.is_lowercase() && b.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(prompt: &str) -> Vec<&'static str> {
        lint(prompt).into_iter().map(|f| f.code).collect()
    }

    #[test]
    fn conversational_replies_are_not_flagged() {
        for p in [
            "yes",
            "go ahead",
            "looks good, thanks!",
            "what does this do?",
        ] {
            assert!(codes(p).is_empty(), "{p}");
        }
    }

    #[test]
    fn slash_commands_are_skipped() {
        assert!(codes("/fix it").is_empty());
    }

    #[test]
    fn vague_task_is_flagged() {
        assert!(codes("fix it").contains(&"vague-target"));
        assert!(codes("can you fix the bug please").contains(&"vague-target"));
    }

    #[test]
    fn anchors_suppress_vague_target() {
        for p in [
            "fix the bug in src/auth.rs",
            "fix it in `parse_line`",
            "fix the issue in parseConfig",
            "fix this on line 42",
            "fix the error \"connection refused\"",
            "fix it in main.go",
            "fix it in Stats::scan",
        ] {
            assert!(!codes(p).contains(&"vague-target"), "{p}");
        }
    }

    #[test]
    fn generic_requests_without_pronouns_are_vague() {
        for p in [
            "add logging",
            "fix the crash",
            "improve the code",
            "update the docs",
        ] {
            assert!(codes(p).contains(&"vague-target"), "{p}");
        }
    }

    #[test]
    fn symptom_statements_are_tasks() {
        assert!(codes("it's not working").contains(&"vague-target"));
        assert!(codes("the build is red").contains(&"vague-target"));
    }

    #[test]
    fn domain_words_make_a_target_specific() {
        for p in [
            "fix the bug where the cart total goes negative with a 100% coupon",
            "make the dashboard load faster, it fetches every order on page load",
        ] {
            assert!(!codes(p).contains(&"vague-target"), "{p}");
        }
    }

    #[test]
    fn questions_and_continuations_are_not_flagged() {
        for p in [
            "why did you change that?",
            "fix it the same way you fixed the previous one",
        ] {
            assert!(!codes(p).contains(&"vague-target"), "{p}");
        }
        // A request phrased with "can you" is still a request.
        assert!(codes("can you fix this?").contains(&"vague-target"));
    }

    #[test]
    fn long_descriptions_are_not_vague() {
        let p = "fix it ".to_string() + &"so the dashboard loads quickly for every user ".repeat(5);
        assert!(!codes(&p).contains(&"vague-target"));
    }

    #[test]
    fn done_criteria_is_info_and_suppressed_by_markers() {
        let p = "refactor the payment module into smaller pieces";
        let findings = lint(p);
        assert_eq!(findings[0].code, "no-done-criteria");
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(codes("refactor the payment module, tests must still pass").is_empty());
    }

    #[test]
    fn large_paste_is_flagged_even_without_task_verb() {
        let p = "line of log output\n".repeat(2_000);
        assert_eq!(codes(&p), ["large-paste"]);
    }
}
