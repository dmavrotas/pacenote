//! pacenote: a co-driver for Claude Code.
//!
//! - [`stats`]: token-usage analytics from Claude Code session transcripts
//! - [`lint`]: heuristic prompt linting ([`lang`]: per-language word packs)
//! - [`hook`]: Claude Code hook handlers
//! - [`install`]: (un)registering the hook in settings.json

pub mod hook;
pub mod install;
pub mod lang;
pub mod lint;
pub mod report;
pub mod stats;
pub mod transcript;
