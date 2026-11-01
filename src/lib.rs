//! pacenote: a co-driver for Claude Code.
//!
//! - [`stats`]: token-usage analytics from Claude Code session transcripts
//! - [`lint`]: heuristic prompt linting
//! - [`hook`]: Claude Code hook handlers

pub mod hook;
pub mod lint;
pub mod report;
pub mod stats;
pub mod transcript;
