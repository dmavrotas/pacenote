//! The coding agents pacenote knows about.

use std::path::PathBuf;

use anyhow::{Context, Result};
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
            Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
            Agent::Codex => ("CODEX_HOME", ".codex"),
            Agent::Cursor => ("", ".cursor"),
        };
        if let Some(path) = std::env::var_os(var).filter(|_| !var.is_empty()) {
            return Ok(PathBuf::from(path));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("neither HOME nor USERPROFILE is set")?;
        Ok(PathBuf::from(home).join(dir))
    }

    /// Where transcripts live by default.
    pub fn transcript_root(self) -> Result<PathBuf> {
        let sub = match self {
            Agent::Claude => "projects",
            Agent::Codex => "sessions",
            Agent::Cursor => "projects",
        };
        Ok(self.config_dir()?.join(sub))
    }
}
