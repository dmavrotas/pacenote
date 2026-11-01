use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use pacenote::hook;
use pacenote::install::{self, Change};
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
    /// Register the prompt hook in Claude Code's settings.json
    Install {
        #[command(flatten)]
        target: SettingsTarget,
        /// Have the hook include informational findings
        #[arg(long)]
        strict: bool,
        /// Hook command to register (default: this binary's absolute path)
        #[arg(long)]
        command: Option<String>,
    },
    /// Remove the prompt hook from Claude Code's settings.json
    Uninstall {
        #[command(flatten)]
        target: SettingsTarget,
    },
}

#[derive(clap::Args)]
struct SettingsTarget {
    /// Which settings file to edit
    #[arg(long, value_enum, default_value_t = Scope::User)]
    scope: Scope,
    /// Edit this settings file instead of the one chosen by --scope
    #[arg(long, value_name = "PATH", conflicts_with = "scope")]
    settings: Option<PathBuf>,
    /// Print the resulting settings without writing them
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Scope {
    /// ~/.claude/settings.json (all projects)
    User,
    /// .claude/settings.json in the current directory (shared with the team)
    Project,
    /// .claude/settings.local.json in the current directory (just you)
    Local,
}

impl SettingsTarget {
    fn path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.settings {
            return Ok(path.clone());
        }
        Ok(match self.scope {
            Scope::User => install::claude_config_dir()?.join("settings.json"),
            Scope::Project => PathBuf::from(".claude/settings.json"),
            Scope::Local => PathBuf::from(".claude/settings.local.json"),
        })
    }
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
        Command::Hook {
            event:
                HookEvent::Install {
                    target,
                    strict,
                    command,
                },
        } => {
            let command = match command {
                Some(c) => c,
                None => {
                    let exe = std::env::current_exe().context("locating the pacenote binary")?;
                    warn_if_build_dir(&exe);
                    install::hook_command(&exe, strict)
                }
            };
            let path = target.path()?;
            let mut settings = install::load(&path)?;
            let change = install::install(&mut settings, &command)?;
            let verb = match change {
                Change::Added => "Added",
                Change::Updated => "Updated",
                Change::Unchanged => {
                    println!("pacenote hook already installed in {}", path.display());
                    return Ok(());
                }
            };
            commit(
                &path,
                &settings,
                target.dry_run,
                &format!("{verb} `{command}`"),
            )?;
        }
        Command::Hook {
            event: HookEvent::Uninstall { target },
        } => {
            let path = target.path()?;
            let mut settings = install::load(&path)?;
            match install::uninstall(&mut settings) {
                0 => println!("No pacenote hook found in {}", path.display()),
                n => commit(
                    &path,
                    &settings,
                    target.dry_run,
                    &format!("Removed {n} pacenote hook(s)"),
                )?,
            }
        }
    }
    Ok(())
}

fn default_root() -> Result<PathBuf> {
    Ok(install::claude_config_dir()?.join("projects"))
}

/// Write (or, for a dry run, print) the edited settings and report what happened.
fn commit(path: &Path, settings: &serde_json::Value, dry_run: bool, what: &str) -> Result<()> {
    if dry_run {
        println!("{}", serde_json::to_string_pretty(settings)?);
        eprintln!("(dry run) {what} in {}", path.display());
        return Ok(());
    }
    let backup = install::save(path, settings)?;
    println!("{what} in {}", path.display());
    if let Some(backup) = backup {
        println!("Previous settings saved to {}", backup.display());
    }
    Ok(())
}

fn warn_if_build_dir(exe: &Path) {
    let components: Vec<_> = exe.components().map(|c| c.as_os_str().to_owned()).collect();
    if components
        .windows(2)
        .any(|w| w[0] == "target" && (w[1] == "debug" || w[1] == "release"))
    {
        eprintln!(
            "warning: registering a binary inside a cargo build directory ({}).\n\
             It will break on `cargo clean`; run `cargo install --path .` for a stable path.",
            exe.display()
        );
    }
}

fn read_stdin() -> Result<String> {
    if io::stdin().is_terminal() {
        anyhow::bail!("pass the prompt as an argument or pipe it on stdin");
    }
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    Ok(text)
}
