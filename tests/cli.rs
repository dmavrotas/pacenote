//! End-to-end tests of the `pacenote` binary.

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn pacenote() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pacenote"))
}

fn write_jsonl(path: &std::path::Path, lines: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: Vec<String> = lines.iter().map(Value::to_string).collect();
    fs::write(path, body.join("\n") + "\n").unwrap();
}

fn assistant(req: &str, input: u64, content: Value) -> Value {
    json!({"type": "assistant", "requestId": req, "timestamp": "2026-10-05T12:00:00Z",
           "message": {"model": "claude-opus-5-5", "content": content,
                       "usage": {"input_tokens": input, "output_tokens": 10,
                                 "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 0}}})
}

#[test]
fn stats_json_over_fixture_tree() {
    let root = tempfile::tempdir().unwrap();
    let bash = json!([{"type": "tool_use", "id": "t1", "name": "Bash",
                       "input": {"command": "cd app && cargo test 2>&1"}}]);
    write_jsonl(
        &root.path().join("-home-dev-demo/s1.jsonl"),
        &[
            json!({"type": "mode", "mode": "normal"}),
            assistant("req_a", 5, json!([{"type": "thinking"}])),
            assistant("req_a", 5, bash), // same request, second content block
            json!({"type": "user", "timestamp": "2026-10-05T12:00:01Z", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "x".repeat(800)}]}}),
        ],
    );
    // Subagent transcript nested below the session directory.
    write_jsonl(
        &root
            .path()
            .join("-home-dev-demo/s1/subagents/agent-1.jsonl"),
        &[assistant("req_b", 7, json!([]))],
    );
    write_jsonl(
        &root.path().join("-home-dev-other/s2.jsonl"),
        &[assistant("req_c", 100, json!([]))],
    );

    let out = pacenote()
        .args(["stats", "--json", "--project", "demo", "--root"])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(v["files_scanned"], 2);
    assert_eq!(v["totals"]["api_calls"], 2);
    assert_eq!(v["totals"]["input_tokens"], 12);
    assert_eq!(v["tools"]["Bash: cargo test"]["bytes"], 800);
    assert!(v["by_project"].get("-home-dev-other").is_none());
}

#[test]
fn stats_text_report_renders() {
    let root = tempfile::tempdir().unwrap();
    write_jsonl(
        &root.path().join("-p/s.jsonl"),
        &[assistant("r", 1, json!([]))],
    );
    let out = pacenote()
        .args(["stats", "--root"])
        .arg(root.path())
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("TOTALS"), "{text}");
    assert!(text.contains("claude-opus-5-5"));
}

#[test]
fn stats_missing_root_fails_with_context() {
    let out = pacenote()
        .args(["stats", "--root", "/definitely/not/here"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("reading transcripts"));
}

#[test]
fn hook_prompt_round_trip() {
    let run = |payload: &str| {
        let mut child = pacenote()
            .args(["hook", "prompt"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "hooks must always exit 0");
        String::from_utf8(out.stdout).unwrap()
    };

    let vague = run(r#"{"hook_event_name":"UserPromptSubmit","prompt":"fix it"}"#);
    let v: Value = serde_json::from_str(&vague).unwrap();
    assert!(
        v["systemMessage"]
            .as_str()
            .unwrap()
            .contains("vague-target")
    );

    assert_eq!(run(r#"{"prompt":"fix the panic in src/stats.rs"}"#), "");
    assert_eq!(run("garbage"), "");
}

#[test]
fn lint_command() {
    let out = pacenote().args(["lint", "fix the bug"]).output().unwrap();
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("vague-target")
    );
}
