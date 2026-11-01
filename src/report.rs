//! Human-readable rendering of [`Stats`].

use std::fmt::Write;

use crate::stats::{Stats, Totals};

/// Flag tools whose results average more than this many tokens.
const NOISY_TOOL_AVG_TOKENS: u64 = 2_000;
const LOW_CACHE_HIT_RATE: f64 = 0.80;

pub fn human(n: u64) -> String {
    match n {
        n if n >= 1_000_000_000 => format!("{:.1}B", n as f64 / 1e9),
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.1}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

/// Turn `-Users-me-DioCode-pacenote` into `DioCode-pacenote` when it lives
/// under the user's home directory.
pub fn display_project(raw: &str, home: Option<&str>) -> String {
    if let Some(home) = home {
        let encoded = home.replace(['/', '.'], "-");
        if let Some(rest) = raw.strip_prefix(&encoded) {
            let rest = rest.trim_start_matches('-');
            return if rest.is_empty() {
                "~".into()
            } else {
                rest.into()
            };
        }
    }
    raw.to_string()
}

pub fn render(stats: &Stats, home: Option<&str>, top: usize) -> String {
    let mut out = String::new();
    let t = &stats.totals;
    let u = &t.usage;

    let _ = writeln!(
        out,
        "pacenote · {} API calls across {} transcript files\n",
        human(t.api_calls),
        stats.files_scanned
    );
    let _ = writeln!(out, "TOTALS");
    for (label, value) in [
        ("input (uncached)", u.input_tokens),
        ("cache writes", u.cache_creation_input_tokens),
        ("cache reads", u.cache_read_input_tokens),
        ("output", u.output_tokens),
    ] {
        let _ = writeln!(out, "  {label:<18}{:>10}", human(value));
    }
    let _ = writeln!(
        out,
        "  {:<18}{:>9.1}%\n",
        "cache hit rate",
        u.cache_hit_rate() * 100.0
    );

    let _ = writeln!(out, "BY MODEL");
    usage_table(
        &mut out,
        stats.by_model.iter().map(|(k, v)| (k.clone(), v)),
        usize::MAX,
    );

    let _ = writeln!(out, "BY PROJECT (top {top} by input)");
    usage_table(
        &mut out,
        stats
            .by_project
            .iter()
            .map(|(k, v)| (display_project(k, home), v)),
        top,
    );

    let tools = stats.tools_by_size();
    if !tools.is_empty() {
        let _ = writeln!(out, "TOOL OUTPUT ENTERING CONTEXT (top {top}, est. tokens)");
        let _ = writeln!(
            out,
            "  {:<32}{:>8}{:>10}{:>9}{:>9}",
            "tool", "calls", "tokens", "avg", "max"
        );
        for (label, tool) in tools.iter().take(top) {
            let _ = writeln!(
                out,
                "  {:<32}{:>8}{:>10}{:>9}{:>9}",
                truncate(label, 31),
                human(tool.calls),
                human(tool.est_tokens()),
                human(tool.avg_tokens()),
                human(tool.max_bytes / crate::stats::BYTES_PER_TOKEN),
            );
        }
        let _ = writeln!(out);
    }

    let hints = hints(stats);
    if !hints.is_empty() {
        let _ = writeln!(out, "HINTS");
        for hint in hints {
            let _ = writeln!(out, "  • {hint}");
        }
    }
    if stats.malformed_lines > 0 {
        let _ = writeln!(
            out,
            "\n({} unparseable lines skipped)",
            human(stats.malformed_lines)
        );
    }
    out
}

fn usage_table<'a>(out: &mut String, rows: impl Iterator<Item = (String, &'a Totals)>, top: usize) {
    let mut rows: Vec<_> = rows.collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1.usage.total_input()));
    let _ = writeln!(
        out,
        "  {:<32}{:>8}{:>10}{:>10}{:>9}{:>8}",
        "", "calls", "input", "output", "cached", "hit%"
    );
    for (name, t) in rows.into_iter().take(top) {
        let _ = writeln!(
            out,
            "  {:<32}{:>8}{:>10}{:>10}{:>9}{:>7.1}%",
            truncate(&name, 31),
            human(t.api_calls),
            human(t.usage.total_input()),
            human(t.usage.output_tokens),
            human(t.usage.cache_read_input_tokens),
            t.usage.cache_hit_rate() * 100.0,
        );
    }
    let _ = writeln!(out);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

pub fn hints(stats: &Stats) -> Vec<String> {
    let mut hints = Vec::new();
    let usage = &stats.totals.usage;
    if usage.total_input() > 0 && usage.cache_hit_rate() < LOW_CACHE_HIT_RATE {
        hints.push(format!(
            "Cache hit rate is {:.0}%. Frequent /clear, model switches, or editing CLAUDE.md \
             mid-session all invalidate the cache.",
            usage.cache_hit_rate() * 100.0
        ));
    }
    for (label, tool) in stats.tools_by_size().into_iter().take(5) {
        if tool.avg_tokens() > NOISY_TOOL_AVG_TOKENS {
            hints.push(format!(
                "`{label}` averages ~{} tokens per result. Filter its output (quiet flags, \
                 `| tail`, an output-condensing wrapper) or read narrower ranges.",
                human(tool.avg_tokens())
            ));
        }
    }
    hints
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn human_numbers() {
        assert_eq!(human(999), "999");
        assert_eq!(human(1_500), "1.5k");
        assert_eq!(human(2_340_000), "2.3M");
        assert_eq!(human(7_000_000_000), "7.0B");
    }

    #[test]
    fn project_names_strip_home_prefix() {
        let home = Some("/Users/me");
        assert_eq!(display_project("-Users-me-DioCode", home), "DioCode");
        assert_eq!(display_project("-Users-me", home), "~");
        assert_eq!(display_project("-opt-work", home), "-opt-work");
        assert_eq!(display_project("-Users-me-x", None), "-Users-me-x");
    }

    #[test]
    fn hints_flag_noisy_tools_and_low_cache() {
        let mut stats = Stats::default();
        stats.ingest_line(
            "p",
            &json!({"type": "assistant", "requestId": "r",
                    "message": {"model": "m", "usage": {"input_tokens": 100, "cache_read_input_tokens": 10},
                                "content": [{"type": "tool_use", "id": "t", "name": "Bash",
                                             "input": {"command": "npm test"}}]}})
            .to_string(),
        );
        stats.ingest_line(
            "p",
            &json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t", "content": "x".repeat(40_000)}]}})
            .to_string(),
        );
        let hints = hints(&stats);
        assert!(hints.iter().any(|h| h.contains("Cache hit rate")));
        assert!(hints.iter().any(|h| h.contains("`Bash: npm test`")));

        let text = render(&stats, None, 10);
        assert!(text.contains("TOOL OUTPUT ENTERING CONTEXT"));
        assert!(text.contains("Bash: npm test"));
    }
}
