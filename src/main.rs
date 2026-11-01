use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use pacenote::hook;
use pacenote::lint::{Severity, lint};
use pacenote::report;
use pacenote::stats::{self, Filter};

#[derive(Parser)]
#[command(
    name = "pacenote",
    version,
    about = "A co-driver for Claude Code: spend fewer tokens, waste fewer turns"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize token usage from Claude Code transcripts
    Stats {
        /// Transcript root (default: $CLAUDE_CONFIG_DIR/projects or ~/.claude/projects)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Only include projects whose directory name contains this text
        #[arg(long, short)]
        project: Option<String>,
        /// Only include activity on or after this date (YYYY-MM-DD)
        #[arg(long, value_name = "DATE")]
        since: Option<String>,
        /// Rows to show per table
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Emit JSON instead of a report
        #[arg(long)]
        json: bool,
    },
    /// Lint a prompt given as an argument or on stdin
    Lint {
        text: Option<String>,
        /// Include informational findings
        #[arg(long)]
        strict: bool,
    },
    /// Claude Code hook handlers (configured in settings.json)
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
}

#[derive(Subcommand)]
enum HookEvent {
    /// UserPromptSubmit: warn about vague or oversized prompts (adds no tokens)
    Prompt {
        /// Include informational findings
        #[arg(long)]
        strict: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Stats {
            root,
            project,
            since,
            top,
            json,
        } => {
            let root = root.map_or_else(default_root, Ok)?;
            let stats = stats::scan(&root, Filter { since, project })
                .with_context(|| format!("reading transcripts under {}", root.display()))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                let home = std::env::var("HOME").ok();
                print!("{}", report::render(&stats, home.as_deref(), top));
            }
        }
        Command::Lint { text, strict } => {
            let text = match text {
                Some(t) => t,
                None => read_stdin()?,
            };
            let findings: Vec<_> = lint(&text)
                .into_iter()
                .filter(|f| strict || f.severity >= Severity::Warn)
                .collect();
            if findings.is_empty() {
                println!("No findings.");
            } else {
                println!("{}", hook::format_findings(&findings));
            }
        }
        Command::Hook {
            event: HookEvent::Prompt { strict },
        } => {
            // A hook must never break the user's session: swallow all errors.
            let mut payload = String::new();
            if io::stdin().read_to_string(&mut payload).is_ok() {
                let min = if strict {
                    Severity::Info
                } else {
                    Severity::Warn
                };
                if let Some(out) = hook::user_prompt_submit(&payload, min) {
                    println!("{out}");
                }
            }
        }
    }
    Ok(())
}

fn default_root() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("projects"));
    }
    let home = std::env::var("HOME").context("HOME is not set; pass --root")?;
    Ok(PathBuf::from(home).join(".claude").join("projects"))
}

fn read_stdin() -> Result<String> {
    if io::stdin().is_terminal() {
        anyhow::bail!("pass the prompt as an argument or pipe it on stdin");
    }
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    Ok(text)
}
