//! Parsing of Claude Code session transcripts
//! (`~/.claude/projects/<project>/<session>.jsonl`).
//!
//! The format is undocumented and evolves between Claude Code versions, so
//! parsing is deliberately lenient: unknown fields are ignored and lines that
//! don't parse are reported as malformed rather than aborting the scan.
//!
//! One API response is written as several transcript lines (one per content
//! block), each repeating the same `usage`. Consumers must deduplicate
//! [`Event::ApiCall`] by `request_id`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token counts reported by the API for one request.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_creation_input_tokens += other.cache_creation_input_tokens;
        self.cache_read_input_tokens += other.cache_read_input_tokens;
    }

    /// Every input-side token: uncached + cache writes + cache reads.
    pub fn total_input(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }

    /// Share of input tokens served from the prompt cache (0.0–1.0).
    pub fn cache_hit_rate(&self) -> f64 {
        match self.total_input() {
            0 => 0.0,
            total => self.cache_read_input_tokens as f64 / total as f64,
        }
    }
}

/// The facts pacenote extracts from a transcript line.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A model response with its token usage.
    ApiCall {
        request_id: String,
        model: String,
        timestamp: String,
        usage: Usage,
    },
    /// Claude invoked a tool; `label` groups similar calls (e.g. `Bash: cargo test`).
    ToolUse { id: String, label: String },
    /// A tool's output, which enters the context window.
    ToolResult {
        id: String,
        bytes: usize,
        timestamp: String,
    },
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    #[serde(default)]
    timestamp: String,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    #[serde(default)]
    model: String,
    usage: Option<Usage>,
    #[serde(default)]
    content: Value,
}

/// Parse one JSONL line. Returns `None` if the line is not valid JSON of the
/// expected shape; returns an empty vec for lines that carry nothing of interest.
pub fn parse_line(line: &str) -> Option<Vec<Event>> {
    let line: Line = serde_json::from_str(line).ok()?;
    let mut events = Vec::new();
    let Some(msg) = line.message else {
        return Some(events);
    };

    match line.kind.as_str() {
        "assistant" => {
            let request_id = line.request_id.or(msg.id);
            if let (Some(usage), Some(request_id)) = (msg.usage, request_id) {
                // Claude Code writes locally generated messages with a placeholder model.
                if msg.model != "<synthetic>" {
                    events.push(Event::ApiCall {
                        request_id,
                        model: msg.model,
                        timestamp: line.timestamp.clone(),
                        usage,
                    });
                }
            }
            for block in blocks(&msg.content) {
                if block["type"] != "tool_use" {
                    continue;
                }
                if let (Some(id), Some(name)) = (block["id"].as_str(), block["name"].as_str()) {
                    events.push(Event::ToolUse {
                        id: id.to_string(),
                        label: tool_label(name, &block["input"]),
                    });
                }
            }
        }
        "user" => {
            for block in blocks(&msg.content) {
                if block["type"] != "tool_result" {
                    continue;
                }
                if let Some(id) = block["tool_use_id"].as_str() {
                    events.push(Event::ToolResult {
                        id: id.to_string(),
                        bytes: text_len(&block["content"]),
                        timestamp: line.timestamp.clone(),
                    });
                }
            }
        }
        _ => {}
    }
    Some(events)
}

fn blocks(content: &Value) -> &[Value] {
    content.as_array().map(Vec::as_slice).unwrap_or(&[])
}

/// Length of the textual part of a tool result (string, or array of text parts).
fn text_len(content: &Value) -> usize {
    match content {
        Value::String(s) => s.len(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .map(str::len)
            .sum(),
        _ => 0,
    }
}

/// Group key for a tool call. Bash calls are grouped by program (and
/// subcommand for multi-command tools), since that's where noisy output hides.
pub fn tool_label(name: &str, input: &Value) -> String {
    if name != "Bash" {
        return name.to_string();
    }
    let command = input.get("command").and_then(Value::as_str).unwrap_or("");
    format!("Bash: {}", command_label(command))
}

/// Tools whose first non-flag argument is a meaningful subcommand.
const SUBCOMMAND_TOOLS: &[&str] = &[
    "git",
    "cargo",
    "npm",
    "pnpm",
    "yarn",
    "go",
    "docker",
    "kubectl",
    "gradlew",
    "gradle",
    "mvn",
    "dotnet",
    "terraform",
    "gh",
    "rtk",
    "make",
    "uv",
    "pip",
    "brew",
];

/// Global flags that consume the next token (`git -C dir`, `make -f file`).
const FLAGS_WITH_VALUE: &[&str] = &["-C", "-c", "-f", "--manifest-path", "--prefix"];

/// Wrappers to look past when finding the real program.
const WRAPPERS: &[&str] = &["sudo", "time", "env", "command", "exec", "nice"];

/// Reduce a shell command to a short label, e.g.
/// `cd app && RUST_LOG=debug cargo test -q 2>&1` → `cargo test`.
pub fn command_label(command: &str) -> String {
    let Some((program, mut tokens)) = command.split(['&', ';', '|', '\n']).find_map(|segment| {
        let mut tokens = segment
            .split_whitespace()
            .skip_while(|t| t.contains('=') || WRAPPERS.contains(t));
        // Skip setup-only segments: `cd dir`, `export X=1`, bare `X=$(...)`.
        match tokens.next() {
            Some("cd" | "export") | None => None,
            // Leftovers of `X=$(cmd ...)` substitutions aren't programs.
            Some(t) if t.starts_with('$') || t.contains(['(', ')']) => None,
            Some(program) => Some((program, tokens)),
        }
    }) else {
        return "(empty)".to_string();
    };
    let program = program.rsplit('/').next().unwrap_or(program);

    if SUBCOMMAND_TOOLS.contains(&program) {
        while let Some(token) = tokens.next() {
            if FLAGS_WITH_VALUE.contains(&token) {
                tokens.next();
            } else if !token.starts_with('-') {
                return format!("{program} {token}");
            }
        }
    }
    program.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assistant(request_id: &str, model: &str, content: Value) -> String {
        json!({
            "type": "assistant",
            "requestId": request_id,
            "timestamp": "2026-10-01T10:00:00Z",
            "message": {
                "id": "msg_1",
                "model": model,
                "usage": {"input_tokens": 3, "output_tokens": 50,
                          "cache_creation_input_tokens": 100, "cache_read_input_tokens": 900},
                "content": content
            }
        })
        .to_string()
    }

    #[test]
    fn parses_api_call_usage() {
        let events = parse_line(&assistant("req_1", "claude-opus-5-5", json!([]))).unwrap();
        assert_eq!(
            events,
            vec![Event::ApiCall {
                request_id: "req_1".into(),
                model: "claude-opus-5-5".into(),
                timestamp: "2026-10-01T10:00:00Z".into(),
                usage: Usage {
                    input_tokens: 3,
                    output_tokens: 50,
                    cache_creation_input_tokens: 100,
                    cache_read_input_tokens: 900,
                },
            }]
        );
    }

    #[test]
    fn ignores_synthetic_messages() {
        let events = parse_line(&assistant("req_1", "<synthetic>", json!([]))).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn extracts_tool_use_with_label() {
        let content = json!([{"type": "tool_use", "id": "toolu_1", "name": "Bash",
                              "input": {"command": "cargo test --all"}}]);
        let events = parse_line(&assistant("req_1", "m", content)).unwrap();
        assert!(events.contains(&Event::ToolUse {
            id: "toolu_1".into(),
            label: "Bash: cargo test".into()
        }));
    }

    #[test]
    fn measures_tool_result_string_and_parts() {
        let line = json!({
            "type": "user", "timestamp": "t",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "a", "content": "12345"},
                {"type": "tool_result", "tool_use_id": "b",
                 "content": [{"type": "text", "text": "abc"}, {"type": "image"}, {"type": "text", "text": "de"}]}
            ]}
        })
        .to_string();
        let events = parse_line(&line).unwrap();
        assert_eq!(
            events,
            vec![
                Event::ToolResult {
                    id: "a".into(),
                    bytes: 5,
                    timestamp: "t".into()
                },
                Event::ToolResult {
                    id: "b".into(),
                    bytes: 5,
                    timestamp: "t".into()
                },
            ]
        );
    }

    #[test]
    fn plain_user_prompt_yields_nothing() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        assert_eq!(parse_line(line), Some(vec![]));
    }

    #[test]
    fn malformed_line_is_none() {
        assert_eq!(parse_line("{not json"), None);
    }

    #[test]
    fn command_labels() {
        let cases = [
            ("cargo test", "cargo test"),
            ("cd /x && cargo build --release 2>&1 | tail", "cargo build"),
            ("RUST_LOG=debug ./target/debug/app --flag", "app"),
            ("sudo docker ps -a", "docker ps"),
            ("ls -la /tmp", "ls"),
            ("git -c color.ui=never log --oneline", "git log"),
            ("rtk git status", "rtk git"),
            ("C=~/cache; J=$(which javap)\nfind $C -name x", "find"),
            ("export PATH=/x:$PATH; cargo clippy", "cargo clippy"),
            ("   ", "(empty)"),
        ];
        for (cmd, want) in cases {
            assert_eq!(command_label(cmd), want, "command: {cmd}");
        }
    }

    #[test]
    fn non_bash_tools_keep_their_name() {
        assert_eq!(tool_label("Read", &json!({"file_path": "/x"})), "Read");
    }

    #[test]
    fn cache_hit_rate() {
        let u = Usage {
            input_tokens: 10,
            cache_creation_input_tokens: 10,
            cache_read_input_tokens: 80,
            output_tokens: 5,
        };
        assert!((u.cache_hit_rate() - 0.8).abs() < 1e-9);
        assert_eq!(Usage::default().cache_hit_rate(), 0.0);
    }
}
