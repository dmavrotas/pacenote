//! Claude Code hook handlers.
//!
//! Hooks must never get in the user's way: any malformed input results in no
//! output, and feedback goes to the user through `systemMessage`, which is
//! not added to Claude's context, so the hook costs zero tokens.
//!
//! With rewriting enabled, a prompt with a warn-level finding is *held*
//! (blocked) and a rewrite is shown instead. Resubmitting the original prompt
//! unchanged within [`HOLD_WINDOW_SECS`] sends it as-is. The hook only blocks
//! when it has a rewrite to offer; on any rewrite failure it falls back to
//! the plain warning.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;
use serde_json::json;

use crate::lint::{Finding, Severity, detect_language, lint};

/// How long a held prompt can be resubmitted unchanged to send it as-is.
pub const HOLD_WINDOW_SECS: u64 = 600;

#[derive(Deserialize)]
struct PromptSubmit {
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

/// Handle a `UserPromptSubmit` payload. Returns the JSON to print on stdout,
/// or `None` when there is nothing to say.
pub fn user_prompt_submit(payload: &str, min_severity: Severity) -> Option<String> {
    let input: PromptSubmit = serde_json::from_str(payload).ok()?;
    let findings = findings(&input.prompt, min_severity);
    warning(&findings)
}

/// Rewrite-enabled variant. `rewrite(prompt, cwd)` produces the suggestion;
/// `state_dir` remembers held prompts; `now` is the current Unix time.
pub fn user_prompt_submit_with_rewrite(
    payload: &str,
    min_severity: Severity,
    rewrite: impl FnOnce(&str, &Path) -> Result<String>,
    state_dir: &Path,
    now: u64,
) -> Option<String> {
    let input: PromptSubmit = serde_json::from_str(payload).ok()?;
    let findings = findings(&input.prompt, min_severity);
    let held_codes: Vec<&str> = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .map(|f| f.code)
        .collect();
    if held_codes.is_empty() {
        return warning(&findings);
    }

    let hold = HoldState::new(state_dir, &input.session_id);
    if hold.take_if_resubmitted(&input.prompt, now) {
        return None; // the user chose to send it as-is
    }

    let cwd = input.cwd.unwrap_or_else(|| PathBuf::from("."));
    match rewrite(&input.prompt, &cwd) {
        Ok(text) if text.trim() != input.prompt.trim() => {
            hold.remember(&input.prompt, now);
            let messages = &detect_language(&input.prompt).messages;
            let reason = format!(
                "{}\n\n{}\n\n{}",
                messages.held.replace("{codes}", &held_codes.join(", ")),
                text.trim(),
                messages.hold_hint
            );
            Some(json!({ "decision": "block", "reason": reason }).to_string())
        }
        Ok(_) => warning(&findings),
        Err(e) => {
            let mut message = format_findings(&findings);
            message.push_str(&format!("\n(pacenote rewrite unavailable: {e:#})"));
            Some(json!({ "systemMessage": message }).to_string())
        }
    }
}

fn findings(prompt: &str, min_severity: Severity) -> Vec<Finding> {
    lint(prompt)
        .into_iter()
        .filter(|f| f.severity >= min_severity)
        .collect()
}

fn warning(findings: &[Finding]) -> Option<String> {
    (!findings.is_empty())
        .then(|| json!({ "systemMessage": format_findings(findings) }).to_string())
}

pub fn format_findings(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("pacenote [{}] {}", f.code, f.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The last held prompt of a session, stored as `<hash> <unix time>`.
struct HoldState {
    path: PathBuf,
}

impl HoldState {
    fn new(dir: &Path, session_id: &str) -> Self {
        Self {
            path: dir.join(format!("pacenote-held-{:016x}", hash(session_id))),
        }
    }

    fn remember(&self, prompt: &str, now: u64) {
        let _ = fs::write(&self.path, format!("{:016x} {now}", hash(prompt.trim())));
    }

    /// Was this exact prompt held recently? Clears the record either way.
    fn take_if_resubmitted(&self, prompt: &str, now: u64) -> bool {
        let Ok(record) = fs::read_to_string(&self.path) else {
            return false;
        };
        let _ = fs::remove_file(&self.path);
        let mut parts = record.split_whitespace();
        let same = parts.next() == Some(format!("{:016x}", hash(prompt.trim())).as_str());
        let recent = parts
            .next()
            .and_then(|t| t.parse::<u64>().ok())
            .is_some_and(|held_at| now.saturating_sub(held_at) <= HOLD_WINDOW_SECS);
        same && recent
    }
}

fn hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::Path;

    #[test]
    fn emits_system_message_for_vague_prompt() {
        let out = user_prompt_submit(r#"{"prompt":"fix it","session_id":"s"}"#, Severity::Warn)
            .expect("finding");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(
            v["systemMessage"]
                .as_str()
                .unwrap()
                .contains("vague-target")
        );
        assert!(
            v.get("hookSpecificOutput").is_none(),
            "must not add context tokens"
        );
    }

    #[test]
    fn silent_for_good_prompt_or_bad_payload() {
        let good = r#"{"prompt":"fix the panic in src/stats.rs so `cargo test` passes"}"#;
        assert_eq!(user_prompt_submit(good, Severity::Info), None);
        assert_eq!(user_prompt_submit("not json", Severity::Info), None);
    }

    fn held(payload: &str, dir: &Path, now: u64, rewrite: &str) -> Option<Value> {
        let rewrite = rewrite.to_string();
        user_prompt_submit_with_rewrite(payload, Severity::Warn, |_, _| Ok(rewrite), dir, now)
            .map(|out| serde_json::from_str(&out).unwrap())
    }

    #[test]
    fn rewrite_holds_flagged_prompt_then_releases_unchanged_resubmit() {
        let dir = tempfile::tempdir().unwrap();
        let payload = r#"{"prompt":"fix the bug","session_id":"s1","cwd":"/repo"}"#;

        let first = held(payload, dir.path(), 1_000, "Fix the crash in src/cart.rs").unwrap();
        assert_eq!(first["decision"], "block");
        let reason = first["reason"].as_str().unwrap();
        assert!(reason.contains("vague-target") && reason.contains("Fix the crash in src/cart.rs"));

        // Same prompt again within the window: sent as-is.
        assert_eq!(held(payload, dir.path(), 1_100, "unused"), None);
        // The hold was consumed: a third identical prompt is held again.
        assert_eq!(
            held(payload, dir.path(), 1_200, "again").unwrap()["decision"],
            "block"
        );
    }

    #[test]
    fn hold_expires_and_is_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let s1 = r#"{"prompt":"fix it","session_id":"s1"}"#;
        let s2 = r#"{"prompt":"fix it","session_id":"s2"}"#;
        held(s1, dir.path(), 0, "Fix X").unwrap();
        assert!(
            held(s2, dir.path(), 10, "Fix X").is_some(),
            "other session is unaffected"
        );
        assert!(
            held(s1, dir.path(), HOLD_WINDOW_SECS + 1, "Fix X").is_some(),
            "expired hold"
        );
    }

    #[test]
    fn rewrite_failure_or_no_change_never_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let payload = r#"{"prompt":"fix it","session_id":"s"}"#;
        let failed = user_prompt_submit_with_rewrite(
            payload,
            Severity::Warn,
            |_, _| anyhow::bail!("offline"),
            dir.path(),
            0,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&failed).unwrap();
        assert!(v.get("decision").is_none());
        assert!(
            v["systemMessage"]
                .as_str()
                .unwrap()
                .contains("rewrite unavailable: offline")
        );

        let unchanged = held(payload, dir.path(), 0, "fix it").unwrap();
        assert!(unchanged.get("decision").is_none());
    }

    #[test]
    fn clean_prompts_skip_the_rewriter() {
        let dir = tempfile::tempdir().unwrap();
        let out = user_prompt_submit_with_rewrite(
            r#"{"prompt":"fix the panic in src/stats.rs","session_id":"s"}"#,
            Severity::Warn,
            |_, _| panic!("rewriter must not be called"),
            dir.path(),
            0,
        );
        assert_eq!(out, None);
    }

    #[test]
    fn held_message_uses_prompt_language() {
        let dir = tempfile::tempdir().unwrap();
        let v = held(
            r#"{"prompt":"διόρθωσέ το","session_id":"s"}"#,
            dir.path(),
            0,
            "Διόρθωσε το X",
        )
        .unwrap();
        assert!(v["reason"].as_str().unwrap().contains("κρατήθηκε"));
    }

    #[test]
    fn severity_threshold_filters_info() {
        let p = r#"{"prompt":"refactor the payment module into smaller pieces"}"#;
        assert_eq!(user_prompt_submit(p, Severity::Warn), None);
        assert!(user_prompt_submit(p, Severity::Info).is_some());
    }
}
