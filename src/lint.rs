//! Heuristic prompt linting.
//!
//! The goal is fewer wasted turns, not prettier prompts: a prompt with no
//! concrete target makes Claude spend tool calls (and tokens) searching for
//! it. Rules only fire on prompts that look like task requests, so
//! conversational replies ("yes", "go ahead") are never flagged.

use serde::Serialize;

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

/// Verbs that make a prompt a request to change something.
const TASK_VERBS: &[&str] = &[
    "fix",
    "add",
    "implement",
    "refactor",
    "debug",
    "update",
    "change",
    "make",
    "create",
    "write",
    "build",
    "investigate",
    "optimize",
    "optimise",
    "improve",
    "remove",
    "delete",
    "rename",
    "migrate",
    "support",
    "handle",
    "resolve",
    "clean",
    "speed",
];

/// Symptom statements that imply "fix it" without a verb ("the build is red").
const SYMPTOMS: &[&str] = &[
    "broken",
    "failing",
    "fails",
    "crashes",
    "crashing",
    "red",
    "throws",
    "not working",
    "doesn't work",
    "isn't working",
    "stopped working",
];

/// Words that don't identify *what* to change: pronouns, filler, and nouns
/// so generic they apply to any codebase ("the code", "the tests", "logging").
const GENERIC_WORDS: &[&str] = &[
    // pronouns and determiners
    "it",
    "it's",
    "its",
    "this",
    "that",
    "these",
    "those",
    "them",
    "they",
    "something",
    "everything",
    "anything",
    "whatever",
    "all",
    "some",
    "any",
    "more",
    "the",
    "a",
    "an",
    "i",
    "we",
    "you",
    "me",
    "my",
    "our",
    "your",
    "there",
    "here",
    "one",
    // function words
    "to",
    "of",
    "in",
    "on",
    "for",
    "with",
    "from",
    "up",
    "out",
    "about",
    "and",
    "or",
    "but",
    "so",
    "is",
    "are",
    "was",
    "be",
    "been",
    "not",
    "can",
    "could",
    "would",
    "will",
    "please",
    "just",
    "now",
    "too",
    "very",
    "really",
    "still",
    "anymore",
    "asap",
    "again",
    "do",
    "does",
    "doesn't",
    "did",
    "get",
    "getting",
    "got",
    "take",
    "look",
    "keeps",
    "keep",
    "what",
    "why",
    "how",
    "we've",
    "discussed",
    "talked",
    "mentioned",
    // generic engineering nouns
    "code",
    "codebase",
    "bug",
    "issue",
    "error",
    "exception",
    "warning",
    "problem",
    "crash",
    "test",
    "doc",
    "docs",
    "documentation",
    "comment",
    "app",
    "application",
    "project",
    "repo",
    "feature",
    "ticket",
    "logging",
    "log",
    "handling",
    "build",
    "ci",
    "pipeline",
    "linter",
    "lint",
    "performance",
    "dependency",
    "dependencies",
    "version",
    "stuff",
    "thing",
    "case",
    "edge",
    // generic qualities
    "better",
    "faster",
    "cleaner",
    "robust",
    "properly",
    "correctly",
    "slow",
    "fast",
    "new",
    "wrong",
    "working",
    "work",
    "broken",
    "failing",
    "red",
];

/// References to earlier conversation: the target is already established.
const CONTINUATIONS: &[&str] = &[
    "same",
    "previous",
    "again",
    "above",
    "before",
    "other",
    "rest",
    "remaining",
];

/// First words of a question (as opposed to a request).
const QUESTION_WORDS: &[&str] = &[
    "why", "what", "how", "which", "where", "when", "who", "is", "are", "does", "did",
];

const DONE_MARKERS: &[&str] = &[
    "should",
    "must",
    "expect",
    "so that",
    "until",
    "pass",
    "passes",
    "test",
    "tests",
    "returns",
    "verify",
    "done when",
    "acceptance",
    "without breaking",
];

pub fn lint(prompt: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let trimmed = prompt.trim();
    if trimmed.starts_with('/') || trimmed.starts_with('!') {
        return findings; // slash commands and shell escapes
    }

    let est_tokens = trimmed.len() / 4;
    if est_tokens > LARGE_PASTE_TOKENS {
        findings.push(Finding {
            code: "large-paste",
            severity: Severity::Warn,
            message: format!(
                "~{est_tokens} tokens pasted. Point Claude at a file path instead, or keep only \
                 the relevant part (e.g. the first error and its stack trace)."
            ),
        });
    }

    let words = words(trimmed);
    let phrase = format!(" {} ", words.join(" "));
    let contains = |p: &&str| phrase.contains(&format!(" {p} "));
    let is_task =
        words.iter().any(|w| TASK_VERBS.contains(&w.as_str())) || SYMPTOMS.iter().any(contains);
    if !is_task || is_question(trimmed, &words) {
        return findings;
    }

    if words.len() <= VAGUE_MAX_WORDS
        && !has_anchor(trimmed)
        && !CONTINUATIONS.iter().any(contains)
        && words.iter().all(|w| is_generic(w))
    {
        findings.push(Finding {
            code: "vague-target",
            severity: Severity::Warn,
            message: "No concrete target. Name the file, function, or exact error message so \
                      Claude doesn't spend turns searching for it."
                .to_string(),
        });
    }

    if words.len() >= DONE_CRITERIA_MIN_WORDS && !DONE_MARKERS.iter().any(contains) {
        findings.push(Finding {
            code: "no-done-criteria",
            severity: Severity::Info,
            message: "Say what \"done\" looks like (e.g. \"the tests in auth/ pass\", \
                      \"returns 404 for unknown ids\") so Claude knows when to stop."
                .to_string(),
        });
    }
    findings
}

/// A single question ("why did you change that?") rather than a request.
fn is_question(text: &str, words: &[String]) -> bool {
    text.ends_with('?')
        && text.matches('?').count() == 1
        && words
            .first()
            .is_some_and(|w| QUESTION_WORDS.contains(&w.as_str()))
}

/// Is this word generic (or a task verb), allowing simple inflections?
fn is_generic(word: &str) -> bool {
    let known = |w: &str| GENERIC_WORDS.contains(&w) || TASK_VERBS.contains(&w);
    known(word)
        || ["s", "es", "ed", "d", "ing", "ly"]
            .iter()
            .filter_map(|suffix| word.strip_suffix(suffix))
            .any(known)
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Does the prompt point at something concrete: code, a path, an identifier,
/// a line number, or a quoted error?
fn has_anchor(text: &str) -> bool {
    if text.contains('`') {
        return true;
    }
    if quoted_spans(text).any(|q| q.len() > 3) {
        return true;
    }
    let tokens: Vec<&str> = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| ",;:!?()[]{}'\"".contains(c)))
        .map(|t| t.trim_end_matches('.'))
        .collect();
    tokens.iter().any(|t| is_identifier_like(t))
        || tokens
            .windows(2)
            .any(|w| w[0].eq_ignore_ascii_case("line") && w[1].parse::<u32>().is_ok())
}

fn quoted_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('"').skip(1).step_by(2)
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
    let mut chars = token.chars();
    let first = chars.next();
    first.is_some_and(char::is_alphabetic)
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
