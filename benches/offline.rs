//! Offline benchmark suite. Free, deterministic, no network:
//!
//! ```sh
//! cargo bench --bench offline                    # ~100 MB synthetic corpus
//! PACENOTE_BENCH_MB=500 cargo bench --bench offline
//! ```
//!
//! Sections:
//! 1. **Stats accuracy**: synthetic transcripts with known ground truth,
//!    compared against pacenote and against a naive per-line sum.
//! 2. **Lint quality**: precision/recall of `vague-target` on the labelled
//!    corpus in `bench/prompts.jsonl`.
//! 3. **Scan throughput**: how fast `pacenote stats` reads transcripts.
//! 4. **Hook latency**: the delay `pacenote hook prompt` adds to every prompt.
//!
//! The report is printed and written to `target/bench/offline.md`. Accuracy
//! checks are assertions: the bench fails if pacenote's totals are wrong.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pacenote::lint::lint;
use pacenote::stats::{self, Filter};
use serde::Deserialize;
use serde_json::json;

fn main() {
    let mb: u64 = std::env::var("PACENOTE_BENCH_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let mut report = String::new();
    let _ = writeln!(report, "# pacenote offline benchmark\n");
    let _ = writeln!(
        report,
        "pacenote {} · {}/{} · {} CPUs · corpus {mb} MB\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(1, |n| n.get()),
    );

    let dir = tempfile::tempdir().expect("tempdir");
    eprintln!("generating {mb} MB of synthetic transcripts…");
    let truth = generate(dir.path(), mb * 1_000_000, 0x5eed);

    accuracy(&mut report, dir.path(), &truth);
    lint_quality(&mut report);
    throughput(&mut report, dir.path(), &truth);
    hook_latency(&mut report);

    print!("{report}");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/bench");
    fs::create_dir_all(&out).expect("create target/bench");
    fs::write(out.join("offline.md"), &report).expect("write report");
    eprintln!("report written to {}", out.join("offline.md").display());
}

// ---------------------------------------------------------------------------
// Synthetic transcripts with ground truth
// ---------------------------------------------------------------------------

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

#[derive(Default)]
struct Truth {
    api_calls: u64,
    total_input: u64,
    output: u64,
    tool_results: u64,
    tool_bytes: u64,
    malformed: u64,
    /// What summing `usage` over every assistant line would report.
    naive_api_calls: u64,
    naive_total_input: u64,
    naive_output: u64,
    bytes: u64,
    lines: u64,
    files: u64,
}

const COMMANDS: &[&str] = &[
    "cargo test",
    "cd app && cargo build --release 2>&1",
    "git status",
    "git diff HEAD~1",
    "cat src/main.rs",
    "grep -rn TODO src",
    "npm test",
    "ls -la",
    "python3 -m pytest -q",
    "docker compose logs api",
];
const FILE_BYTES: u64 = 2_000_000;

fn generate(root: &Path, target_bytes: u64, seed: u64) -> Truth {
    let mut rng = Rng(seed);
    let mut truth = Truth::default();
    let output_chunk = "test result line with some typical tool output text\n".repeat(2_000);
    let mut request = 0u64;
    let mut tool = 0u64;

    while truth.bytes < target_bytes {
        let project = root.join(format!("-home-dev-project{}", truth.files % 7));
        fs::create_dir_all(&project).unwrap();
        let path = project.join(format!("session-{}.jsonl", truth.files));
        let mut w = BufWriter::new(File::create(&path).unwrap());
        let file_start = truth.bytes;
        let emit = |w: &mut BufWriter<File>, line: &str, truth: &mut Truth| {
            w.write_all(line.as_bytes()).unwrap();
            w.write_all(b"\n").unwrap();
            truth.bytes += line.len() as u64 + 1;
            truth.lines += 1;
        };

        emit(
            &mut w,
            &json!({"type": "permission-mode", "permissionMode": "default"}).to_string(),
            &mut truth,
        );
        while truth.bytes - file_start < FILE_BYTES {
            emit(
                &mut w,
                &json!({"type": "user", "timestamp": "2026-10-01T10:00:00Z",
                        "message": {"role": "user", "content": "please continue with the next step"}})
                .to_string(),
                &mut truth,
            );

            request += 1;
            let usage = json!({
                "input_tokens": rng.range(1, 10),
                "cache_creation_input_tokens": rng.range(0, 5_000),
                "cache_read_input_tokens": rng.range(10_000, 200_000),
                "output_tokens": rng.range(50, 3_000),
            });
            let total_input = [
                "input_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]
            .iter()
            .map(|k| usage[k].as_u64().unwrap())
            .sum::<u64>();
            let output = usage["output_tokens"].as_u64().unwrap();
            truth.api_calls += 1;
            truth.total_input += total_input;
            truth.output += output;

            // One API response is written as 1–4 lines repeating the same usage.
            let blocks = rng.range(1, 4);
            let tool_id = rng.chance(60).then(|| {
                tool += 1;
                format!("toolu_{tool}")
            });
            for b in 0..blocks {
                let last = b == blocks - 1;
                let content = match (&tool_id, last) {
                    (Some(id), true) => json!([{"type": "tool_use", "id": id, "name": "Bash",
                        "input": {"command": COMMANDS[(rng.next() % COMMANDS.len() as u64) as usize]}}]),
                    _ if b == 0 => {
                        json!([{"type": "thinking", "thinking": "considering the next step"}])
                    }
                    _ => json!([{"type": "text", "text": "Here is what I found."}]),
                };
                emit(
                    &mut w,
                    &json!({"type": "assistant", "requestId": format!("req_{request}"),
                            "timestamp": "2026-10-01T10:00:01Z",
                            "message": {"id": format!("msg_{request}"), "model": "claude-opus-5-5",
                                        "usage": usage, "content": content}})
                    .to_string(),
                    &mut truth,
                );
                truth.naive_api_calls += 1;
                truth.naive_total_input += total_input;
                truth.naive_output += output;
            }

            if let Some(id) = tool_id {
                // Heavy-tailed result sizes, like real tool output.
                let size = match rng.next() % 100 {
                    0..70 => rng.range(100, 2_000),
                    70..95 => rng.range(2_000, 20_000),
                    _ => rng.range(20_000, 100_000),
                } as usize;
                let text = &output_chunk[..size.min(output_chunk.len())];
                truth.tool_results += 1;
                truth.tool_bytes += text.len() as u64;
                emit(
                    &mut w,
                    &json!({"type": "user", "timestamp": "2026-10-01T10:00:02Z",
                            "message": {"role": "user", "content": [
                                {"type": "tool_result", "tool_use_id": id, "content": text}]}})
                    .to_string(),
                    &mut truth,
                );
            }

            if rng.chance(1) {
                emit(
                    &mut w,
                    "{\"type\":\"assistant\",\"message\":{truncated",
                    &mut truth,
                );
                truth.malformed += 1;
            }
            if rng.chance(1) {
                // Locally generated message: carries usage but is not an API call.
                emit(
                    &mut w,
                    &json!({"type": "assistant", "requestId": format!("synthetic_{request}"),
                            "message": {"model": "<synthetic>", "content": [],
                                        "usage": {"input_tokens": 0, "output_tokens": 0}}})
                    .to_string(),
                    &mut truth,
                );
                truth.naive_api_calls += 1;
            }
        }
        truth.files += 1;
    }
    truth
}

// ---------------------------------------------------------------------------
// 1. Accuracy
// ---------------------------------------------------------------------------

fn accuracy(report: &mut String, root: &Path, truth: &Truth) {
    let stats = stats::scan(root, Filter::default()).expect("scan");
    let tool_results: u64 = stats.tools.values().map(|t| t.calls).sum();
    let tool_bytes: u64 = stats.tools.values().map(|t| t.bytes).sum();
    let total_input = stats.totals.usage.total_input();
    let output = stats.totals.usage.output_tokens;

    let _ = writeln!(report, "## 1. Stats accuracy\n");
    let _ = writeln!(
        report,
        "Synthetic transcripts: {} files, {} lines, {} API calls written as 1–4 lines each \
         (as Claude Code does), plus malformed and `<synthetic>` lines.\n",
        truth.files, truth.lines, truth.api_calls
    );
    let _ = writeln!(
        report,
        "| metric | ground truth | pacenote | naive per-line sum |"
    );
    let _ = writeln!(report, "|---|---:|---:|---:|");
    let row = |r: &mut String, name: &str, t: u64, p: u64, n: Option<u64>| {
        let naive = n.map_or("n/a".to_string(), |n| format!("{n} ({})", pct_err(n, t)));
        let _ = writeln!(r, "| {name} | {t} | {p} ({}) | {naive} |", pct_err(p, t));
    };
    row(
        report,
        "API calls",
        truth.api_calls,
        stats.totals.api_calls,
        Some(truth.naive_api_calls),
    );
    row(
        report,
        "input tokens (all)",
        truth.total_input,
        total_input,
        Some(truth.naive_total_input),
    );
    row(
        report,
        "output tokens",
        truth.output,
        output,
        Some(truth.naive_output),
    );
    row(
        report,
        "tool results",
        truth.tool_results,
        tool_results,
        None,
    );
    row(
        report,
        "tool result bytes",
        truth.tool_bytes,
        tool_bytes,
        None,
    );
    row(
        report,
        "malformed lines",
        truth.malformed,
        stats.malformed_lines,
        None,
    );
    let _ = writeln!(report);

    assert_eq!(stats.totals.api_calls, truth.api_calls, "API call count");
    assert_eq!(total_input, truth.total_input, "input tokens");
    assert_eq!(output, truth.output, "output tokens");
    assert_eq!(tool_results, truth.tool_results, "tool results");
    assert_eq!(tool_bytes, truth.tool_bytes, "tool bytes");
    assert_eq!(stats.malformed_lines, truth.malformed, "malformed lines");
}

fn pct_err(value: u64, truth: u64) -> String {
    if value == truth {
        return "exact".to_string();
    }
    format!(
        "{:+.1}%",
        (value as f64 - truth as f64) / truth as f64 * 100.0
    )
}

// ---------------------------------------------------------------------------
// 2. Lint quality
// ---------------------------------------------------------------------------

/// Languages with prompt corpora in `bench/` (`prompts[.<lang>].jsonl`).
const LANGUAGES: &[&str] = &["en", "el", "fr", "es", "de"];

#[derive(Clone, Deserialize)]
struct Labelled {
    category: String,
    vague: bool,
    prompt: String,
}

fn corpus() -> Vec<Labelled> {
    load_prompts("prompts.jsonl")
}

fn load_prompts(name: &str) -> Vec<Labelled> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("bench")
        .join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid corpus line"))
        .collect()
}

fn flags_vague(prompt: &str) -> bool {
    lint(prompt).iter().any(|f| f.code == "vague-target")
}

fn lint_quality(report: &mut String) {
    let _ = writeln!(report, "## 2. Lint quality (`vague-target`)\n");
    let _ = writeln!(
        report,
        "Everyday engineering prompts, human-labelled by \"would Claude have to guess or go \
         searching for what to change?\", independent of the rule's implementation. The rule \
         was developed against the **dev** set; the **held-out** set was written before the \
         current rule and never used to tune it.\n"
    );
    let _ = writeln!(
        report,
        "| language · set | prompts | precision | recall | F1 | TP | FP | FN | TN |"
    );
    let _ = writeln!(report, "|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    let sets: Vec<(String, Vec<Labelled>)> = LANGUAGES
        .iter()
        .flat_map(|lang| {
            let suffix = if *lang == "en" {
                String::new()
            } else {
                format!(".{lang}")
            };
            [
                (
                    format!("{lang} · dev"),
                    load_prompts(&format!("prompts{suffix}.jsonl")),
                ),
                (
                    format!("{lang} · held-out"),
                    load_prompts(&format!("prompts-holdout{suffix}.jsonl")),
                ),
            ]
        })
        .collect();
    let mut details = String::new();
    for (name, corpus) in &sets {
        let (mut tp, mut fp, mut fn_, mut tn) = (0, 0, 0, 0);
        let mut misses = Vec::new();
        for item in corpus {
            match (flags_vague(&item.prompt), item.vague) {
                (true, true) => tp += 1,
                (true, false) => {
                    fp += 1;
                    misses.push(("false positive", item));
                }
                (false, true) => {
                    fn_ += 1;
                    misses.push(("missed", item));
                }
                (false, false) => tn += 1,
            }
        }
        let precision = ratio(tp, tp + fp);
        let recall = ratio(tp, tp + fn_);
        let f1 = if precision + recall > 0.0 {
            2.0 * precision * recall / (precision + recall)
        } else {
            0.0
        };
        let _ = writeln!(
            report,
            "| {name} | {} | {:.0}% | {:.0}% | {:.2} | {tp} | {fp} | {fn_} | {tn} |",
            corpus.len(),
            precision * 100.0,
            recall * 100.0,
            f1
        );
        if !misses.is_empty() {
            let _ = writeln!(
                details,
                "<details><summary>Misclassified, {name} ({})</summary>\n",
                misses.len()
            );
            for (kind, item) in misses {
                let _ = writeln!(details, "- {kind} ({}): `{}`", item.category, item.prompt);
            }
            let _ = writeln!(details, "\n</details>\n");
        }
    }
    let _ = writeln!(report);

    let pooled = |kind: &str| -> Vec<Labelled> {
        sets.iter()
            .filter(|(name, _)| name.ends_with(kind))
            .flat_map(|(_, corpus)| corpus.iter().cloned())
            .collect()
    };
    let _ = writeln!(report, "All languages pooled:\n");
    let _ = writeln!(report, "| category | dev flagged | held-out flagged |");
    let _ = writeln!(report, "|---|---:|---:|");
    for category in ["vague", "specific", "conversational", "hard"] {
        let cell = |corpus: &[Labelled]| {
            let items: Vec<_> = corpus.iter().filter(|i| i.category == category).collect();
            let flagged = items.iter().filter(|i| flags_vague(&i.prompt)).count();
            format!("{flagged} / {}", items.len())
        };
        let _ = writeln!(
            report,
            "| {category} | {} | {} |",
            cell(&pooled("dev")),
            cell(&pooled("held-out"))
        );
    }
    let _ = writeln!(report, "\n{details}");
}

fn ratio(n: u32, d: u32) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}

// ---------------------------------------------------------------------------
// 3. Throughput
// ---------------------------------------------------------------------------

fn throughput(report: &mut String, root: &Path, truth: &Truth) {
    const RUNS: usize = 5;
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(stats::scan(root, Filter::default()).expect("scan"));
            start.elapsed()
        })
        .collect();
    times.sort();
    let median = times[RUNS / 2];
    let secs = median.as_secs_f64();

    let corpus = corpus();
    let iterations = 2_000;
    let start = Instant::now();
    for _ in 0..iterations {
        for item in &corpus {
            std::hint::black_box(lint(&item.prompt));
        }
    }
    let lint_secs = start.elapsed().as_secs_f64();
    let lints = (iterations * corpus.len()) as f64;

    let _ = writeln!(report, "## 3. Throughput\n");
    let _ = writeln!(report, "| operation | median of {RUNS} | rate |");
    let _ = writeln!(report, "|---|---:|---:|");
    let _ = writeln!(
        report,
        "| `stats` scan, {:.0} MB / {} lines | {:.0} ms | {:.0} MB/s · {:.2}M lines/s |",
        truth.bytes as f64 / 1e6,
        truth.lines,
        secs * 1e3,
        truth.bytes as f64 / 1e6 / secs,
        truth.lines as f64 / 1e6 / secs,
    );
    let _ = writeln!(
        report,
        "| `lint`, in-process | {:.2} µs/prompt | {:.0}k prompts/s |\n",
        lint_secs / lints * 1e6,
        lints / lint_secs / 1e3,
    );
}

// ---------------------------------------------------------------------------
// 4. Hook latency
// ---------------------------------------------------------------------------

fn hook_latency(report: &mut String) {
    const RUNS: usize = 200;
    let bin = env!("CARGO_BIN_EXE_pacenote");
    let payload = json!({"hook_event_name": "UserPromptSubmit", "session_id": "bench",
                         "prompt": "fix the bug in the pagination code, it skips items"})
    .to_string();

    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            let mut child = Command::new(bin)
                .args(["hook", "prompt"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .expect("spawn pacenote");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
            start.elapsed()
        })
        .collect();
    times.sort();
    let at = |q: f64| times[((RUNS as f64 * q) as usize).min(RUNS - 1)].as_secs_f64() * 1e3;

    let _ = writeln!(report, "## 4. Hook latency\n");
    let _ = writeln!(
        report,
        "End-to-end cost of `pacenote hook prompt` per submitted prompt (process spawn + \
         parse + lint + output), {RUNS} runs.\n"
    );
    let _ = writeln!(report, "| p50 | p95 | p99 | max |");
    let _ = writeln!(report, "|---:|---:|---:|---:|");
    let _ = writeln!(
        report,
        "| {:.1} ms | {:.1} ms | {:.1} ms | {:.1} ms |\n",
        at(0.50),
        at(0.95),
        at(0.99),
        times[RUNS - 1].as_secs_f64() * 1e3
    );
}
