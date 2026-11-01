//! Claude Code hook handlers.
//!
//! Hooks must never get in the user's way: any malformed input results in no
//! output, and feedback goes to the user through `systemMessage`, which is
//! not added to Claude's context, so the hook costs zero tokens.

use serde::Deserialize;
use serde_json::json;

use crate::lint::{Finding, Severity, lint};

#[derive(Deserialize)]
struct PromptSubmit {
    #[serde(default)]
    prompt: String,
}

/// Handle a `UserPromptSubmit` payload. Returns the JSON to print on stdout,
/// or `None` when there is nothing to say.
pub fn user_prompt_submit(payload: &str, min_severity: Severity) -> Option<String> {
    let input: PromptSubmit = serde_json::from_str(payload).ok()?;
    let findings: Vec<Finding> = lint(&input.prompt)
        .into_iter()
        .filter(|f| f.severity >= min_severity)
        .collect();
    if findings.is_empty() {
        return None;
    }
    Some(json!({ "systemMessage": format_findings(&findings) }).to_string())
}

pub fn format_findings(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("pacenote [{}] {}", f.code, f.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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

    #[test]
    fn severity_threshold_filters_info() {
        let p = r#"{"prompt":"refactor the payment module into smaller pieces"}"#;
        assert_eq!(user_prompt_submit(p, Severity::Warn), None);
        assert!(user_prompt_submit(p, Severity::Info).is_some());
    }
}
