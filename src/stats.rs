//! Aggregation of transcript events into token-usage statistics.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::transcript::{Event, Usage, parse_line};

/// Rough bytes-per-token ratio for English text and code. Good enough to rank
/// tools; use the API's token-counting endpoint when exact numbers matter.
pub const BYTES_PER_TOKEN: u64 = 4;

#[derive(Debug, Default, Clone, Serialize)]
pub struct Totals {
    pub api_calls: u64,
    #[serde(flatten)]
    pub usage: Usage,
}

impl Totals {
    fn record(&mut self, usage: &Usage) {
        self.api_calls += 1;
        self.usage.add(usage);
    }
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ToolTotals {
    pub calls: u64,
    pub bytes: u64,
    pub max_bytes: u64,
}

impl ToolTotals {
    pub fn est_tokens(&self) -> u64 {
        self.bytes / BYTES_PER_TOKEN
    }

    pub fn avg_tokens(&self) -> u64 {
        self.est_tokens().checked_div(self.calls).unwrap_or(0)
    }
}

#[derive(Debug, Default, Clone)]
pub struct Filter {
    /// Only count events at or after this ISO-8601 prefix (e.g. `2026-10-01`).
    pub since: Option<String>,
    /// Case-insensitive substring match on the project directory name.
    pub project: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub totals: Totals,
    pub by_model: BTreeMap<String, Totals>,
    pub by_project: BTreeMap<String, Totals>,
    pub tools: BTreeMap<String, ToolTotals>,
    pub files_scanned: u64,
    pub malformed_lines: u64,
    #[serde(skip)]
    filter: Filter,
    #[serde(skip)]
    seen_requests: HashSet<String>,
    #[serde(skip)]
    pending_tools: HashMap<String, String>,
}

impl Stats {
    pub fn new(filter: Filter) -> Self {
        Self {
            filter,
            ..Self::default()
        }
    }

    fn in_window(&self, timestamp: &str) -> bool {
        self.filter
            .since
            .as_deref()
            .is_none_or(|since| timestamp >= since)
    }

    pub fn ingest_line(&mut self, project: &str, line: &str) {
        let Some(events) = parse_line(line) else {
            self.malformed_lines += 1;
            return;
        };
        for event in events {
            match event {
                Event::ApiCall {
                    request_id,
                    model,
                    timestamp,
                    usage,
                } => {
                    if !self.in_window(&timestamp) || !self.seen_requests.insert(request_id) {
                        continue;
                    }
                    self.totals.record(&usage);
                    self.by_model.entry(model).or_default().record(&usage);
                    self.by_project
                        .entry(project.to_string())
                        .or_default()
                        .record(&usage);
                }
                Event::ToolUse { id, label } => {
                    self.pending_tools.insert(id, label);
                }
                Event::ToolResult {
                    id,
                    bytes,
                    timestamp,
                } => {
                    let label = self
                        .pending_tools
                        .remove(&id)
                        .unwrap_or_else(|| "(unknown)".to_string());
                    if !self.in_window(&timestamp) {
                        continue;
                    }
                    let bytes = bytes as u64;
                    let tool = self.tools.entry(label).or_default();
                    tool.calls += 1;
                    tool.bytes += bytes;
                    tool.max_bytes = tool.max_bytes.max(bytes);
                }
            }
        }
    }

    /// Ingest a whole transcript. Invalid UTF-8 is replaced rather than fatal.
    pub fn ingest_reader(&mut self, project: &str, mut reader: impl BufRead) -> io::Result<()> {
        self.files_scanned += 1;
        let mut buf = Vec::new();
        loop {
            buf.clear();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                return Ok(());
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.trim();
            if !line.is_empty() {
                self.ingest_line(project, line);
            }
        }
    }

    /// Tools sorted by estimated tokens, largest first.
    pub fn tools_by_size(&self) -> Vec<(&str, &ToolTotals)> {
        let mut tools: Vec<_> = self.tools.iter().map(|(k, v)| (k.as_str(), v)).collect();
        tools.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then(a.0.cmp(b.0)));
        tools
    }
}

/// Scan every `*.jsonl` under `root/<project>/` (recursively, so subagent
/// transcripts are included).
pub fn scan(root: &Path, filter: Filter) -> io::Result<Stats> {
    let mut stats = Stats::new(filter.clone());
    let needle = filter.project.as_deref().map(str::to_lowercase);

    let mut projects: Vec<_> = fs::read_dir(root)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();
    projects.sort_by_key(|e| e.file_name());

    for project in projects {
        let name = project.file_name().to_string_lossy().into_owned();
        if let Some(needle) = &needle {
            if !name.to_lowercase().contains(needle) {
                continue;
            }
        }
        let mut files = Vec::new();
        collect_jsonl(&project.path(), &mut files)?;
        files.sort();
        for file in files {
            stats.ingest_reader(&name, BufReader::new(File::open(&file)?))?;
        }
    }
    Ok(stats)
}

fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_jsonl(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "jsonl") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn api_line(request_id: &str, ts: &str, input: u64) -> String {
        json!({"type": "assistant", "requestId": request_id, "timestamp": ts,
               "message": {"model": "m", "usage": {"input_tokens": input, "output_tokens": 1},
                           "content": []}})
        .to_string()
    }

    #[test]
    fn deduplicates_repeated_request_lines() {
        let mut stats = Stats::default();
        for _ in 0..3 {
            stats.ingest_line("p", &api_line("req_1", "2026-10-01", 10));
        }
        stats.ingest_line("p", &api_line("req_2", "2026-10-01", 5));
        assert_eq!(stats.totals.api_calls, 2);
        assert_eq!(stats.totals.usage.input_tokens, 15);
        assert_eq!(stats.by_project["p"].api_calls, 2);
        assert_eq!(stats.by_model["m"].usage.output_tokens, 2);
    }

    #[test]
    fn since_filter_excludes_older_events() {
        let mut stats = Stats::new(Filter {
            since: Some("2026-10-02".into()),
            ..Filter::default()
        });
        stats.ingest_line("p", &api_line("old", "2026-10-01T23:59:59Z", 10));
        stats.ingest_line("p", &api_line("new", "2026-10-02T00:00:01Z", 7));
        assert_eq!(stats.totals.usage.input_tokens, 7);
    }

    #[test]
    fn attributes_tool_results_to_their_tool() {
        let mut stats = Stats::default();
        stats.ingest_line(
            "p",
            &json!({"type": "assistant", "requestId": "r",
                    "message": {"model": "m", "usage": {}, "content": [
                        {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo test"}},
                        {"type": "tool_use", "id": "t2", "name": "Read", "input": {}}]}})
            .to_string(),
        );
        for (id, len) in [("t1", 4000), ("t2", 400), ("orphan", 8)] {
            stats.ingest_line(
                "p",
                &json!({"type": "user", "message": {"content": [
                    {"type": "tool_result", "tool_use_id": id, "content": "x".repeat(len)}]}})
                .to_string(),
            );
        }
        let ranked: Vec<_> = stats.tools_by_size().iter().map(|(k, _)| *k).collect();
        assert_eq!(ranked, ["Bash: cargo test", "Read", "(unknown)"]);
        assert_eq!(stats.tools["Bash: cargo test"].est_tokens(), 1000);
    }

    #[test]
    fn counts_malformed_lines() {
        let mut stats = Stats::default();
        stats.ingest_line("p", "garbage");
        assert_eq!(stats.malformed_lines, 1);
    }

    #[test]
    fn ingest_reader_survives_invalid_utf8_and_blank_lines() {
        let mut data = api_line("r1", "t", 1).into_bytes();
        data.extend_from_slice(b"\n\n\xff\xfe\n");
        data.extend_from_slice(api_line("r2", "t", 2).as_bytes());
        let mut stats = Stats::default();
        stats.ingest_reader("p", &data[..]).unwrap();
        assert_eq!(stats.totals.api_calls, 2);
        assert_eq!(stats.malformed_lines, 1);
    }
}
