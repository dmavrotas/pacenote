//! pacenote: a co-driver for Claude Code.
//!
//! - [`stats`]: token-usage analytics from Claude Code session transcripts
//! - [`lint`]: heuristic prompt linting ([`lang`]: per-language word packs)
//! - [`hook`]: Claude Code hook handlers
//! - [`install`]: (un)registering the hook in settings.json
//! - [`ground`]: free local rewriting (trim pastes, name the likely file)
//! - [`rewrite`]: opt-in LLM rewriting with the user's own Claude access

pub mod ground;
pub mod hook;
pub mod install;
pub mod lang;
pub mod lint;
pub mod report;
pub mod rewrite;
pub mod stats;
pub mod transcript;
