//! Parsing of OpenAI Codex CLI rollouts
//! (`$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl`, default
//! `~/.codex`).
//!
//! Every line is `{"timestamp", "type", "payload"}`. Unlike Claude Code
//! transcripts, the model and working directory are not repeated on each
//! line, so parsing is stateful: use one [`CodexParser`] per rollout file.
//!
//! The format is undocumented. This parser follows the layout described in
//! public issue reports and is deliberately lenient: unknown line types and
//! fields are ignored, and only lines that are not JSON objects are
//! reported as malformed.

use serde_json::Value;

use super::{Event, Usage, command_label};

/// `token_count` events this close to the start of a rollout are the parent's
/// history replayed into a forked session, not new API calls.
const REPLAY_WINDOW_SECS: i64 = 5;

/// Tool names whose arguments carry a shell command.
const SHELL_TOOLS: &[&str] = &["shell", "shell_command", "exec_command", "local_shell"];

/// Incremental parser for one Codex rollout file.
#[derive(Debug)]
pub struct CodexParser {
    session: String,
    project: Option<String>,
    model: String,
    started: Option<i64>,
    last_total: Option<u64>,
}

impl CodexParser {
    /// `session` identifies the rollout (its file name); it keeps request ids
    /// unique across files.
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            project: None,
            model: "unknown".into(),
            started: None,
            last_total: None,
        }
    }

    /// The project (last component of the session's working directory), known
    /// once the `session_meta` line has been parsed.
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }

    /// Parse one JSONL line. `None` means the line is not a JSON object.
    pub fn parse_line(&mut self, line: &str) -> Option<Vec<Event>> {
        let value: Value = serde_json::from_str(line).ok()?;
        if !value.is_object() {
            return None;
        }
        let timestamp = value["timestamp"].as_str().unwrap_or("");
        let payload = &value["payload"];
        let events = match value["type"].as_str().unwrap_or("") {
            "session_meta" => {
                self.started = epoch_secs(timestamp);
                self.project = payload["cwd"].as_str().and_then(basename).map(String::from);
                vec![]
            }
            "turn_context" => {
                if let Some(model) = payload["model"].as_str() {
                    self.model = model.to_string();
                }
                vec![]
            }
            "event_msg" if payload["type"] == "token_count" => self.token_count(payload, timestamp),
            "response_item" => tool_event(payload, timestamp).into_iter().collect(),
            _ => vec![],
        };
        Some(events)
    }

    fn token_count(&mut self, payload: &Value, timestamp: &str) -> Vec<Event> {
        let info = &payload["info"];
        if !info.is_object() {
            return vec![]; // the first emission of a session carries no usage
        }
        // Codex re-emits the same cumulative total; count each step once.
        let total = info["total_token_usage"]["total_tokens"].as_u64();
        if total.is_some() && total == self.last_total {
            return vec![];
        }
        self.last_total = total;

        let replayed = matches!(
            (self.started, epoch_secs(timestamp)),
            (Some(start), Some(now)) if now - start <= REPLAY_WINDOW_SECS
        );
        let last = &info["last_token_usage"];
        if replayed || !last.is_object() {
            return vec![];
        }
        let request_id = match total {
            Some(total) => format!("{}#{total}", self.session),
            None => format!("{}@{timestamp}", self.session),
        };
        vec![Event::ApiCall {
            request_id,
            model: self.model.clone(),
            timestamp: timestamp.to_string(),
            usage: usage(last),
        }]
    }
}

/// Map OpenAI's usage onto pacenote's Anthropic-shaped [`Usage`]: OpenAI's
/// `input_tokens` already includes the cached part, and `output_tokens`
/// already includes reasoning. Codex has no cache-write charge.
fn usage(last: &Value) -> Usage {
    let get = |key: &str| last[key].as_u64().unwrap_or(0);
    let cached = get("cached_input_tokens");
    Usage {
        input_tokens: get("input_tokens").saturating_sub(cached),
        cache_read_input_tokens: cached,
        cache_creation_input_tokens: 0,
        output_tokens: get("output_tokens"),
    }
}

fn tool_event(payload: &Value, timestamp: &str) -> Option<Event> {
    match payload["type"].as_str()? {
        "function_call" => {
            let name = payload["name"].as_str()?;
            let args: Value = payload["arguments"]
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(Value::Null);
            Some(Event::ToolUse {
                id: payload["call_id"].as_str()?.to_string(),
                label: tool_label(name, &args),
            })
        }
        "local_shell_call" => Some(Event::ToolUse {
            id: payload["call_id"]
                .as_str()
                .or_else(|| payload["id"].as_str())?
                .to_string(),
            label: shell_label(&payload["action"]),
        }),
        "custom_tool_call" => Some(Event::ToolUse {
            id: payload["call_id"].as_str()?.to_string(),
            label: payload["name"].as_str()?.to_string(),
        }),
        "function_call_output" | "custom_tool_call_output" => Some(Event::ToolResult {
            id: payload["call_id"].as_str()?.to_string(),
            bytes: output_len(&payload["output"]),
            timestamp: timestamp.to_string(),
        }),
        _ => None,
    }
}

fn tool_label(name: &str, args: &Value) -> String {
    if SHELL_TOOLS.contains(&name) {
        shell_label(args)
    } else {
        name.to_string()
    }
}

/// `shell: cargo test` from arguments shaped like `{"command": ["bash", "-lc", "cargo test"]}`,
/// `{"command": "cargo test"}` or `{"cmd": "cargo test"}`.
fn shell_label(args: &Value) -> String {
    let command = match args.get("command").or_else(|| args.get("cmd")) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let parts: Vec<&str> = parts.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [_, "-lc" | "-c", script, ..] => (*script).to_string(),
                _ => parts.join(" "),
            }
        }
        _ => String::new(),
    };
    format!("shell: {}", command_label(&command))
}

/// Size of a tool output: a plain string, a `{content|output}` object, or
/// an array of text parts.
fn output_len(output: &Value) -> usize {
    match output {
        Value::String(s) => s.len(),
        Value::Object(map) => ["content", "output"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str))
            .map_or_else(|| output.to_string().len(), str::len),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .map(str::len)
            .sum(),
        _ => 0,
    }
}

fn basename(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).find(|part| !part.is_empty())
}

/// Seconds since the Unix epoch for `YYYY-MM-DDTHH:MM:SS[.fff]Z`.
fn epoch_secs(timestamp: &str) -> Option<i64> {
    let (date, time) = timestamp.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.trim_end_matches('Z').splitn(3, ':');
    let hour = t.next()?.parse::<i64>().ok()?;
    let minute = t.next()?.parse::<i64>().ok()?;
    let second = t.next()?.split('.').next()?.parse::<i64>().ok()?;
    // Reject out-of-range fields: the input is untrusted, and wild values
    // would overflow the arithmetic below.
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }

    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const START: &str = "2026-11-01T10:00:00.000Z";
    const LATER: &str = "2026-11-01T10:00:30.000Z";

    fn line(kind: &str, ts: &str, payload: Value) -> String {
        json!({"timestamp": ts, "type": kind, "payload": payload}).to_string()
    }

    fn parser() -> CodexParser {
        let mut p = CodexParser::new("rollout-a");
        p.parse_line(&line(
            "session_meta",
            START,
            json!({"id": "a", "cwd": "/Users/me/Code/pacenote"}),
        ));
        p.parse_line(&line(
            "turn_context",
            START,
            json!({"model": "gpt-5-codex"}),
        ));
        p
    }

    fn token_count(ts: &str, total: u64, last_input: u64, cached: u64, output: u64) -> String {
        line(
            "event_msg",
            ts,
            json!({"type": "token_count", "info": {
                "total_token_usage": {"total_tokens": total},
                "last_token_usage": {"input_tokens": last_input, "cached_input_tokens": cached,
                                     "output_tokens": output, "reasoning_output_tokens": 3,
                                     "total_tokens": last_input + output},
                "model_context_window": 272_000}}),
        )
    }

    #[test]
    fn session_meta_sets_project_and_turn_context_sets_model() {
        let mut p = parser();
        assert_eq!(p.project(), Some("pacenote"));
        let events = p
            .parse_line(&token_count(LATER, 1_000, 900, 600, 100))
            .unwrap();
        let [Event::ApiCall { model, .. }] = events.as_slice() else {
            panic!("expected one api call, got {events:?}");
        };
        assert_eq!(model, "gpt-5-codex");
    }

    #[test]
    fn maps_openai_usage_to_pacenote_usage() {
        let mut p = parser();
        let events = p
            .parse_line(&token_count(LATER, 1_000, 900, 600, 100))
            .unwrap();
        assert_eq!(
            events,
            vec![Event::ApiCall {
                request_id: "rollout-a#1000".into(),
                model: "gpt-5-codex".into(),
                timestamp: LATER.into(),
                usage: Usage {
                    input_tokens: 300,
                    cache_read_input_tokens: 600,
                    cache_creation_input_tokens: 0,
                    output_tokens: 100,
                },
            }]
        );
    }

    #[test]
    fn counts_repeated_token_counts_once() {
        let mut p = parser();
        let first = p
            .parse_line(&token_count(LATER, 1_000, 900, 0, 100))
            .unwrap();
        let repeat = p
            .parse_line(&token_count(LATER, 1_000, 900, 0, 100))
            .unwrap();
        let next = p
            .parse_line(&token_count("2026-11-01T10:01:00Z", 1_800, 700, 0, 100))
            .unwrap();
        assert_eq!((first.len(), repeat.len(), next.len()), (1, 0, 1));
    }

    #[test]
    fn ignores_null_info_and_forked_replay() {
        let mut p = parser();
        let null_info = line(
            "event_msg",
            LATER,
            json!({"type": "token_count", "info": null}),
        );
        assert_eq!(p.parse_line(&null_info), Some(vec![]));
        let replay = token_count("2026-11-01T10:00:02Z", 500, 400, 0, 100);
        assert_eq!(p.parse_line(&replay), Some(vec![]));
    }

    #[test]
    fn labels_shell_calls_and_other_tools() {
        let mut p = parser();
        let cases = [
            (
                json!({"type": "function_call", "name": "shell", "call_id": "c1",
                       "arguments": "{\"command\":[\"bash\",\"-lc\",\"cd /x && cargo test -q\"]}"}),
                "shell: cargo test",
            ),
            (
                json!({"type": "function_call", "name": "exec_command", "call_id": "c2",
                       "arguments": "{\"cmd\":\"git status\"}"}),
                "shell: git status",
            ),
            (
                json!({"type": "local_shell_call", "call_id": "c3",
                       "action": {"type": "exec", "command": ["ls", "-la"]}}),
                "shell: ls",
            ),
            (
                json!({"type": "custom_tool_call", "name": "apply_patch", "call_id": "c4",
                       "input": "*** Begin Patch"}),
                "apply_patch",
            ),
            (
                json!({"type": "function_call", "name": "update_plan", "call_id": "c5",
                       "arguments": "{}"}),
                "update_plan",
            ),
        ];
        for (payload, want) in cases {
            let id = payload["call_id"].as_str().unwrap().to_string();
            let events = p
                .parse_line(&line("response_item", LATER, payload))
                .unwrap();
            assert_eq!(
                events,
                vec![Event::ToolUse {
                    id,
                    label: want.into()
                }]
            );
        }
    }

    #[test]
    fn measures_tool_outputs() {
        let mut p = parser();
        let cases = [
            (json!("12345"), 5),
            (json!({"content": "abc", "success": true}), 3),
            (
                json!([{"type": "text", "text": "ab"}, {"type": "text", "text": "cd"}]),
                4,
            ),
        ];
        for (output, want) in cases {
            let payload = json!({"type": "function_call_output", "call_id": "c", "output": output});
            let events = p
                .parse_line(&line("response_item", LATER, payload))
                .unwrap();
            assert_eq!(
                events,
                vec![Event::ToolResult {
                    id: "c".into(),
                    bytes: want,
                    timestamp: LATER.into()
                }]
            );
        }
    }

    #[test]
    fn unknown_lines_are_ignored_and_non_objects_are_malformed() {
        let mut p = parser();
        assert_eq!(
            p.parse_line(&line("compacted", LATER, json!({}))),
            Some(vec![])
        );
        assert_eq!(p.parse_line("{not json"), None);
        assert_eq!(p.parse_line("42"), None);
    }

    #[test]
    fn parses_timestamps() {
        assert_eq!(epoch_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_secs("2026-11-01T10:00:30.250Z"), Some(1_793_527_230));
        assert_eq!(epoch_secs("garbage"), None);
        for hostile in [
            "9999999999999999-01-01T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-11-01T99:00:00Z",
            "-5-01-01T00:00:00Z",
            "2026-11-01T10:00Z",
        ] {
            assert_eq!(epoch_secs(hostile), None, "{hostile}");
        }
    }
}
