//! Opt-in prompt rewriting with the user's own Claude access.
//!
//! Two backends:
//! - **api**: the Claude Messages API with `ANTHROPIC_API_KEY` (raw HTTP; there
//!   is no official Rust SDK);
//! - **claude**: the user's logged-in Claude Code (`claude -p`), so a Claude
//!   subscription works without an API key.
//!
//! The rewriter sees the prompt plus cheap repository context (file list and
//! `git status`) so it can name real files; it is instructed never to invent
//! any. Nothing is sent anywhere unless the user enables rewriting.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

/// Default model; override with `--model` or `PACENOTE_MODEL`.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
/// Set on the `claude -p` child so pacenote's own hook ignores the rewrite
/// request instead of recursing into another rewrite.
pub const REWRITING_ENV: &str = "PACENOTE_REWRITING";

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const MAX_CONTEXT_BYTES: usize = 12_000;

pub const SYSTEM_PROMPT: &str = "\
You rewrite prompts that software engineers send to Claude Code, a coding agent working in their \
repository, so the agent can act with fewer wasted turns.

Rules:
- Keep the engineer's intent and language: reply in the same language as the prompt.
- Make the target concrete. Name files or functions only when the prompt or the repository \
context shows them. Never invent file names, errors, commands or requirements.
- Name a command (test runner, build tool) only if the context shows it is used, for example a \
Makefile, package.json script or CI config. Otherwise describe the step generically, such as \
\"run the test suite\".
- If the target is unknown, say how to find it in one step instead of guessing.
- State what \"done\" looks like when it can be inferred (for example, which tests should pass).
- If the prompt contains a large paste such as a log, keep only the lines that matter (the first \
error and its stack trace) verbatim and drop the rest.
- Keep it short: usually one to four sentences, plus any kept excerpt.
- Do not solve the task and do not add work the engineer did not ask for.
- If the prompt is already specific, return it unchanged.

Output only the rewritten prompt, with no preamble or explanation.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BackendChoice {
    /// API key if `ANTHROPIC_API_KEY` is set, otherwise the `claude` CLI
    Auto,
    /// Claude Messages API with `ANTHROPIC_API_KEY`
    Api,
    /// Your logged-in Claude Code (`claude -p`)
    Claude,
}

#[derive(Debug, Clone)]
enum Backend {
    Api { key: String },
    Claude { exe: String },
}

#[derive(Debug, Clone)]
pub struct Rewriter {
    backend: Backend,
    pub model: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rewrite {
    pub text: String,
    pub backend: &'static str,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Reported by the `claude` backend only.
    pub cost_usd: Option<f64>,
}

impl Rewriter {
    pub fn new(choice: BackendChoice, model: Option<String>, timeout: Duration) -> Result<Self> {
        let model = model
            .or_else(|| std::env::var("PACENOTE_MODEL").ok())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let backend = match (choice, key) {
            (BackendChoice::Api | BackendChoice::Auto, Some(key)) => Backend::Api { key },
            (BackendChoice::Api, None) => bail!("--backend api needs ANTHROPIC_API_KEY"),
            (BackendChoice::Claude | BackendChoice::Auto, _) => Backend::Claude {
                exe: std::env::var("PACENOTE_CLAUDE").unwrap_or_else(|_| "claude".into()),
            },
        };
        Ok(Self {
            backend,
            model,
            timeout,
        })
    }

    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Api { .. } => "api",
            Backend::Claude { .. } => "claude",
        }
    }

    /// Rewrite `prompt`, using `context` (see [`repo_context`]) if non-empty.
    pub fn rewrite(&self, prompt: &str, context: &str) -> Result<Rewrite> {
        let message = user_message(prompt, context);
        match &self.backend {
            Backend::Api { key } => self.via_api(key, &message),
            Backend::Claude { exe } => self.via_claude(exe, &message),
        }
    }

    fn via_api(&self, key: &str, message: &str) -> Result<Rewrite> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let mut request = agent
            .post(API_URL)
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        if supports_fallbacks(&self.model) {
            request = request.header("anthropic-beta", FALLBACK_BETA);
        }
        let mut response = request
            .send_json(request_body(&self.model, message))
            .context("calling the Claude API")?;
        let status = response.status();
        let body: Value = response
            .body_mut()
            .read_json()
            .context("reading the Claude API response")?;
        if !status.is_success() {
            let detail = body["error"]["message"].as_str().unwrap_or("unknown error");
            bail!("Claude API returned {status}: {detail}");
        }
        parse_api_response(&body)
    }

    fn via_claude(&self, exe: &str, message: &str) -> Result<Rewrite> {
        let mut child = Command::new(exe)
            .args([
                "-p",
                "--model",
                &self.model,
                "--output-format",
                "json",
                "--system-prompt",
                SYSTEM_PROMPT,
                "--tools",
                "",
                "--effort",
                "low",
                "--setting-sources",
                "",
                "--strict-mcp-config",
                "--no-session-persistence",
            ])
            .env(REWRITING_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting `{exe}`; is Claude Code installed?"))?;
        child
            .stdin
            .take()
            .expect("piped stdin")
            .write_all(message.as_bytes())?;

        let deadline = Instant::now() + self.timeout;
        while child.try_wait()?.is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                bail!("`{exe} -p` timed out after {}s", self.timeout.as_secs());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut stdout = Vec::new();
        child
            .stdout
            .take()
            .expect("piped stdout")
            .read_to_end(&mut stdout)?;
        let result: Value = serde_json::from_slice(&stdout).map_err(|_| {
            let mut stderr = String::new();
            let _ = child
                .stderr
                .take()
                .map(|mut e| e.read_to_string(&mut stderr));
            anyhow!(
                "`{exe} -p` gave no JSON result: {}",
                stderr.lines().next().unwrap_or("no output")
            )
        })?;
        parse_cli_response(&result)
    }
}

/// Models that accept `fallbacks: "default"`. Claude Haiku 5.5 has no
/// server-side fallback and older models predate the parameter.
fn supports_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

/// Models that accept `output_config.effort`.
fn supports_effort(model: &str) -> bool {
    [
        "claude-opus-5",
        "claude-fable-5",
        "claude-sonnet-5",
        "claude-haiku-5",
        "claude-opus-4-6",
        "claude-opus-4-7",
        "claude-opus-4-8",
        "claude-sonnet-4-6",
    ]
    .iter()
    .any(|prefix| model.starts_with(prefix))
}

pub fn request_body(model: &str, message: &str) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": 16_000,
        "system": SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": message}],
    });
    if supports_effort(model) {
        // Rewriting a prompt is a small task; low effort keeps it fast and cheap.
        body["output_config"] = json!({"effort": "low"});
    }
    if supports_fallbacks(model) {
        // Retry a safety-classifier decline on Anthropic's recommended model.
        body["fallbacks"] = json!("default");
    }
    body
}

fn user_message(prompt: &str, context: &str) -> String {
    let context = if context.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<repository_context>\n{}\n</repository_context>\n\n",
            context.trim()
        )
    };
    format!("{context}<prompt>\n{prompt}\n</prompt>")
}

pub fn parse_api_response(body: &Value) -> Result<Rewrite> {
    match body["stop_reason"].as_str() {
        Some("refusal") => bail!("the model declined to rewrite this prompt"),
        Some("max_tokens") => bail!("the rewrite was cut off (max_tokens)"),
        _ => {}
    }
    let text: String = body["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    let usage = &body["usage"];
    finish(
        text,
        "api",
        usage["input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
        usage["output_tokens"].as_u64().unwrap_or(0),
        None,
    )
}

pub fn parse_cli_response(result: &Value) -> Result<Rewrite> {
    if result["is_error"].as_bool().unwrap_or(true) {
        let detail = result["result"]
            .as_str()
            .or(result["subtype"].as_str())
            .unwrap_or("unknown error");
        bail!("claude -p failed: {detail}");
    }
    let usage = &result["usage"];
    finish(
        result["result"].as_str().unwrap_or_default().to_string(),
        "claude",
        usage["input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0)
            + usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
        usage["output_tokens"].as_u64().unwrap_or(0),
        result["total_cost_usd"].as_f64(),
    )
}

fn finish(
    text: String,
    backend: &'static str,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: Option<f64>,
) -> Result<Rewrite> {
    let text = strip_wrapper(text.trim()).to_string();
    if text.is_empty() {
        bail!("the model returned an empty rewrite");
    }
    Ok(Rewrite {
        text,
        backend,
        input_tokens,
        output_tokens,
        cost_usd,
    })
}

/// Models sometimes echo the `<prompt>` wrapper back; drop it.
fn strip_wrapper(text: &str) -> &str {
    text.strip_prefix("<prompt>")
        .and_then(|t| t.strip_suffix("</prompt>"))
        .map_or(text, str::trim)
}

/// Cheap repository context: name, tracked files and `git status`, capped
/// at ~12 KB. Falls back to a top-level listing outside git.
pub fn repo_context(dir: &Path) -> String {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();

    let mut context = format!("Repository: {name}\n");
    match git(&["ls-files"]) {
        Some(files) => {
            let status = git(&["status", "--short"]).unwrap_or_default();
            if !status.trim().is_empty() {
                context.push_str("\ngit status --short:\n");
                context.push_str(&cap_lines(&status, 40));
            }
            context.push_str("\nTracked files:\n");
            context.push_str(&files);
        }
        None => {
            context.push_str("\nTop-level entries:\n");
            if let Ok(entries) = std::fs::read_dir(dir) {
                let mut names: Vec<_> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.starts_with('.'))
                    .collect();
                names.sort();
                context.push_str(&names.join("\n"));
            }
        }
    }
    truncate_bytes(&context, MAX_CONTEXT_BYTES)
}

fn cap_lines(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = lines
        .iter()
        .take(max)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > max {
        out.push_str(&format!("\n… ({} more)", lines.len() - max));
    }
    out.push('\n');
    out
}

fn truncate_bytes(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let cut = text[..max].rfind('\n').unwrap_or(max);
    format!("{}\n… (truncated)", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_request_has_low_effort_and_default_fallbacks() {
        let body = request_body("claude-opus-5-5", "hi");
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body["fallbacks"], "default");
        assert!(
            body.get("thinking").is_none(),
            "Opus 5.5 rejects disabled thinking"
        );
        assert_eq!(body["system"], SYSTEM_PROMPT);
    }

    #[test]
    fn haiku_request_has_no_fallbacks() {
        let body = request_body("claude-haiku-5-5", "hi");
        assert!(body.get("fallbacks").is_none());
        assert_eq!(body["output_config"]["effort"], "low");
    }

    #[test]
    fn older_models_get_neither_extension() {
        let body = request_body("claude-haiku-4-5", "hi");
        assert!(body.get("fallbacks").is_none());
        assert!(body.get("output_config").is_none());
    }

    #[test]
    fn user_message_wraps_prompt_and_optional_context() {
        assert_eq!(user_message("fix it", ""), "<prompt>\nfix it\n</prompt>");
        let m = user_message("fix it", "Repository: shop");
        assert!(m.starts_with("<repository_context>\nRepository: shop\n</repository_context>"));
        assert!(m.ends_with("<prompt>\nfix it\n</prompt>"));
    }

    #[test]
    fn parses_api_text_and_usage() {
        let body = json!({
            "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": ""},
                        {"type": "text", "text": "  Fix `paginate()` in shop/pagination.py.  "}],
            "usage": {"input_tokens": 10, "cache_read_input_tokens": 5, "output_tokens": 20}
        });
        let r = parse_api_response(&body).unwrap();
        assert_eq!(r.text, "Fix `paginate()` in shop/pagination.py.");
        assert_eq!((r.input_tokens, r.output_tokens), (15, 20));
        assert_eq!(r.backend, "api");
    }

    #[test]
    fn api_refusal_and_empty_text_are_errors() {
        let refusal = json!({"stop_reason": "refusal", "content": []});
        assert!(
            parse_api_response(&refusal)
                .unwrap_err()
                .to_string()
                .contains("declined")
        );
        let empty = json!({"stop_reason": "end_turn", "content": []});
        assert!(parse_api_response(&empty).is_err());
    }

    #[test]
    fn parses_cli_result() {
        let ok = json!({"is_error": false, "result": "<prompt>\nRun the tests\n</prompt>",
                        "total_cost_usd": 0.004, "usage": {"input_tokens": 3, "output_tokens": 9}});
        let r = parse_cli_response(&ok).unwrap();
        assert_eq!(r.text, "Run the tests");
        assert_eq!(r.cost_usd, Some(0.004));
        let failed = json!({"is_error": true, "result": "Not logged in"});
        assert!(
            parse_cli_response(&failed)
                .unwrap_err()
                .to_string()
                .contains("Not logged in")
        );
    }

    #[test]
    fn repo_context_lists_files_and_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        let context = repo_context(dir.path());
        assert!(
            context.contains("main.rs") && context.contains("src"),
            "{context}"
        );
        assert!(truncate_bytes(&"line\n".repeat(10_000), 100).len() < 130);
    }
}
