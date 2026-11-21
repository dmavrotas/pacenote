//! The coding agents pacenote knows about.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Agent {
    /// Claude Code
    Claude,
    /// OpenAI Codex CLI
    Codex,
    /// Cursor
    Cursor,
}

impl Agent {
    /// Where this agent keeps its config: `$CLAUDE_CONFIG_DIR` / `$CODEX_HOME`
    /// when set, else `~/.claude`, `~/.codex`, `~/.cursor`.
    pub fn config_dir(self) -> Result<PathBuf> {
        let (var, dir) = match self {
            Agent::Claude => (Some("CLAUDE_CONFIG_DIR"), ".claude"),
            Agent::Codex => (Some("CODEX_HOME"), ".codex"),
            Agent::Cursor => (None, ".cursor"),
        };
        if let Some(path) = var.and_then(std::env::var_os) {
            return Ok(PathBuf::from(path));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("neither HOME nor USERPROFILE is set")?;
        Ok(PathBuf::from(home).join(dir))
    }

    /// Where transcripts live by default. Cursor has none pacenote can read.
    pub fn transcript_root(self) -> Result<PathBuf> {
        let sub = match self {
            Agent::Claude => "projects",
            Agent::Codex => "sessions",
            Agent::Cursor => {
                bail!("Cursor keeps no readable token data locally; stats are not supported yet")
            }
        };
        Ok(self.config_dir()?.join(sub))
    }
}
