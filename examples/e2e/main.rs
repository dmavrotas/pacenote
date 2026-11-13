//! End-to-end token benchmark. **Spends real tokens** on your Claude account.
//!
//! ```sh
//! cargo run --release --example e2e -- --yes                  # 3 tasks × 2 variants × 3 runs, haiku
//! cargo run --release --example e2e -- --yes --model sonnet --runs 5
//! ```
//!
//! For each everyday engineering task, Claude Code (`claude -p`) is given the
//! same job twice, in fresh copies of `bench/fixture`:
//!
//! - **vague**: how the task is often typed ("fix the failing tests"), which
//!   is the kind of prompt `pacenote lint` flags;
//! - **specific**: the same request after following pacenote's advice (name
//!   the file/symptom, say what done looks like). It contains no information
//!   about the fix itself.
//!
//! Each run is scored by the fixture's own unit tests, so a cheap run that
//! doesn't fix the bug counts as a failure, not a saving. User hooks, plugins
//! and MCP servers are not loaded (`--setting-sources project`,
//! `--strict-mcp-config`), so both variants run in the same environment.

use std::fmt::Write as _;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
mod tasks;

use pacenote::lint::lint;
use pacenote::rewrite::{BackendChoice, Rewriter, repo_context};
use serde::Serialize;
use serde_json::Value;
use tasks::{Split, Task};

#[derive(Parser)]
#[command(about = "End-to-end token benchmark: vague vs specific prompts in Claude Code")]
struct Args {
    /// Runs per task and variant
    #[arg(long, default_value_t = 3)]
    runs: usize,
    /// Model passed to `claude --model`
    #[arg(long, default_value = "haiku")]
    model: String,
    /// Only run tasks whose name contains this text
    #[arg(long)]
    task: Option<String>,
    /// Claude Code executable
    #[arg(long, default_value = "claude")]
    claude: String,
    /// Python executable used by the fixture's tests
    #[arg(long, default_value = "python3")]
    python: String,
    /// Per-run timeout in seconds
    #[arg(long, default_value_t = 600)]
    timeout: u64,
    /// Write raw per-run results as JSON
    #[arg(long, value_name = "PATH")]
    json: Option<PathBuf>,
    /// Which tasks to run
    #[arg(long, value_enum, default_value_t = Split::All)]
    split: Split,
    /// Comma-separated variants: vague, specific, rewrite-llm.
    /// Rewrite variants rewrite the vague prompt; the rewrite's own tokens and
    /// cost are counted against them.
    #[arg(long, value_delimiter = ',', default_value = "vague,specific")]
    variants: Vec<String>,
    /// Only check that the fixture is clean and every task's bug breaks its
    /// tests (free; no Claude calls)
    #[arg(long)]
    check_fixture: bool,
    /// Model for the rewrite (default: pacenote's default)
    #[arg(long)]
    rewrite_model: Option<String>,
    /// Confirm you accept spending tokens
    #[arg(long)]
    yes: bool,
}

#[derive(Serialize)]
struct Run {
    task: &'static str,
    split: &'static str,
    variant: String,
    run: usize,
    success: bool,
    timed_out: bool,
    is_error: bool,
    num_turns: u64,
    input_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    duration_ms: u64,
    /// The rewrite step's own usage (rewrite variants only), included in
    /// `total_tokens()` and `cost_usd`.
    rewrite_tokens: u64,
    rewrite_cost_usd: f64,
    /// The prompt actually sent (shows each rewrite).
    prompt: String,
}

impl Run {
    fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
            + self.output_tokens
            + self.rewrite_tokens
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/fixture");
    let tasks: Vec<Task> = tasks::tasks()
        .into_iter()
        .filter(|t| args.split == Split::All || t.split == args.split)
        .filter(|t| args.task.as_deref().is_none_or(|f| t.name.contains(f)))
        .collect();
    if args.check_fixture {
        return check_fixture(&args, &fixture, &tasks);
    }
    for variant in &args.variants {
        if !VARIANTS.contains(&variant.as_str()) {
            bail!(
                "unknown variant `{variant}`; expected one of {}",
                VARIANTS.join(", ")
            );
        }
    }
    let variants: Vec<&str> = args.variants.iter().map(String::as_str).collect();
    let rewriter = variants
        .contains(&"rewrite-llm")
        .then(|| {
            Rewriter::new(
                BackendChoice::Auto,
                args.rewrite_model.clone(),
                Duration::from_secs(60),
            )
        })
        .transpose()?;
    let sessions = tasks.len() * variants.len() * args.runs;

    if !args.yes {
        bail!(
            "this runs {sessions} Claude Code sessions with model `{}` and spends real tokens.\n\
             Re-run with --yes to proceed.",
            args.model
        );
    }
    check_tool(&args.claude, &["--version"])?;
    check_tool(&args.python, &["--version"])?;
    check_fixture(&args, &fixture, &tasks)?;

    let mut runs = Vec::new();
    for task in &tasks {
        for variant in variants.iter().copied() {
            for run in 1..=args.runs {
                eprint!(
                    "[{}/{sessions}] {} · {variant} · run {run} … ",
                    runs.len() + 1,
                    task.name
                );
                let result = run_once(&args, &fixture, task, variant, rewriter.as_ref(), run)?;
                eprintln!(
                    "{} · {} tokens · {} turns · ${:.4}",
                    if result.success { "pass" } else { "FAIL" },
                    result.total_tokens(),
                    result.num_turns,
                    result.cost_usd
                );
                runs.push(result);
            }
        }
    }

    print!("{}", summarize(&args, &tasks, &variants, &runs));
    if let Some(path) = &args.json {
        fs::write(path, serde_json::to_string_pretty(&runs)?)?;
        eprintln!("raw results written to {}", path.display());
    }
    Ok(())
}

const VARIANTS: &[&str] = &["vague", "specific", "rewrite-llm"];

/// The clean fixture must pass every test, and each task's bugs must break
/// the tests it is scored on.
fn check_fixture(args: &Args, fixture: &Path, tasks: &[Task]) -> Result<()> {
    let clean = tempfile::tempdir()?;
    copy_dir(fixture, clean.path())?;
    if !verify(&args.python, clean.path(), tasks::ALL_TESTS)? {
        bail!("the clean fixture fails its own tests");
    }
    for task in tasks {
        let dir = prepare(fixture, task)?;
        if verify(&args.python, dir.path(), task.verify)? {
            bail!(
                "task `{}`: tests still pass with the bug injected",
                task.name
            );
        }
    }
    eprintln!(
        "fixture ok: clean suite passes; {} task bugs each break their tests",
        tasks.len()
    );
    Ok(())
}

/// The prompt a variant sends, plus the rewrite's own tokens and cost.
fn prompt_for(
    variant: &str,
    task: &Task,
    dir: &Path,
    rewriter: Option<&Rewriter>,
) -> Result<(String, u64, f64)> {
    Ok(match variant {
        "specific" => (task.specific.clone(), 0, 0.0),
        "rewrite-llm" => {
            let rewriter = rewriter.expect("rewriter is built when rewrite-llm is selected");
            let r = rewriter.rewrite(&task.vague, &repo_context(dir))?;
            (
                r.text,
                r.input_tokens + r.output_tokens,
                r.cost_usd.unwrap_or(0.0),
            )
        }
        _ => (task.vague.clone(), 0, 0.0),
    })
}

fn run_once(
    args: &Args,
    fixture: &Path,
    task: &Task,
    variant: &str,
    rewriter: Option<&Rewriter>,
    run: usize,
) -> Result<Run> {
    let dir = prepare(fixture, task)?;
    let start = Instant::now();
    // Rewrites happen inside the fresh copy, exactly as the hook would see it.
    let (prompt, rewrite_tokens, rewrite_cost_usd) =
        prompt_for(variant, task, dir.path(), rewriter)?;
    let prompt = prompt.as_str();
    let mut child = Command::new(&args.claude)
        .args([
            "-p",
            "--model",
            &args.model,
            "--output-format",
            "json",
            "--permission-mode",
            "acceptEdits",
            "--allowedTools",
            "Read,Edit,Write,Glob,Grep,Bash(python3 -m unittest:*),Bash(python -m unittest:*)",
            "--setting-sources",
            "project",
            "--strict-mcp-config",
            "--no-session-persistence",
        ])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting `{}`", args.claude))?;
    // The prompt goes over stdin: large pastes exceed Windows' command-line limit.
    child.stdin.take().unwrap().write_all(prompt.as_bytes())?;

    let deadline = start + Duration::from_secs(args.timeout);
    let mut timed_out = false;
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        sleep(Duration::from_millis(200));
    }
    let output = child.wait_with_output()?;
    let result: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    let usage = &result["usage"];
    let n = |v: &Value| v.as_u64().unwrap_or(0);

    Ok(Run {
        task: task.name,
        split: if task.split == Split::Dev {
            "dev"
        } else {
            "held-out"
        },
        variant: variant.to_string(),
        run,
        success: !timed_out && verify(&args.python, dir.path(), task.verify)?,
        timed_out,
        is_error: result["is_error"].as_bool().unwrap_or(true),
        num_turns: n(&result["num_turns"]),
        input_tokens: n(&usage["input_tokens"]),
        cache_creation_input_tokens: n(&usage["cache_creation_input_tokens"]),
        cache_read_input_tokens: n(&usage["cache_read_input_tokens"]),
        output_tokens: n(&usage["output_tokens"]),
        cost_usd: result["total_cost_usd"].as_f64().unwrap_or(0.0) + rewrite_cost_usd,
        duration_ms: start.elapsed().as_millis() as u64,
        rewrite_tokens,
        rewrite_cost_usd,
        prompt: prompt.to_string(),
    })
}

/// Fresh copy of the fixture with the task's bugs injected and its log, if any.
fn prepare(fixture: &Path, task: &Task) -> Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    copy_dir(fixture, dir.path())?;
    for bug in task.bugs {
        let path = dir.path().join(bug.file);
        let source = fs::read_to_string(&path)?;
        if source.matches(bug.from).count() != 1 {
            bail!(
                "task `{}`: bug pattern not found exactly once in {}",
                task.name,
                bug.file
            );
        }
        fs::write(&path, source.replace(bug.from, bug.to))?;
    }
    if let Some(log) = &task.log {
        fs::create_dir_all(dir.path().join("logs"))?;
        fs::write(dir.path().join("logs/app.log"), log)?;
    }
    Ok(dir)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if entry.file_name() == "__pycache__" {
                continue;
            }
            fs::create_dir_all(&target)?;
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn verify(python: &str, dir: &Path, targets: &[&str]) -> Result<bool> {
    let status = Command::new(python)
        .args(["-m", "unittest", "-q"])
        .args(targets)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("running `{python} -m unittest`"))?;
    Ok(status.success())
}

fn check_tool(exe: &str, args: &[&str]) -> Result<()> {
    Command::new(exe)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("`{exe}` not found; pass its path with the matching flag"))?;
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    match values.len() {
        0 => 0.0,
        n if n % 2 == 1 => values[n / 2],
        n => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

fn summarize(args: &Args, tasks: &[Task], variants: &[&str], runs: &[Run]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "\n# pacenote end-to-end benchmark\n");
    let rewrite_note = if variants.contains(&"rewrite-llm") {
        format!(
            " · rewrites by `{}`",
            args.rewrite_model
                .as_deref()
                .unwrap_or(pacenote::rewrite::DEFAULT_MODEL)
        )
    } else {
        String::new()
    };
    let _ = writeln!(
        out,
        "Claude Code `{}` · model `{}`{rewrite_note} · {} runs per variant · medians shown\n",
        claude_version(&args.claude),
        args.model,
        args.runs
    );
    let _ = writeln!(
        out,
        "| task | variant | pacenote lint | success | tokens | turns | cost | time |"
    );
    let _ = writeln!(out, "|---|---|---|---:|---:|---:|---:|---:|");

    // (tokens, cost, successes, runs) per variant
    let mut totals = vec![(0.0, 0.0, 0usize, 0usize); variants.len()];
    for task in tasks {
        let mut medians = Vec::new();
        for (i, variant) in variants.iter().enumerate() {
            let rs: Vec<&Run> = runs
                .iter()
                .filter(|r| r.task == task.name && r.variant == *variant)
                .collect();
            let tokens = median(rs.iter().map(|r| r.total_tokens() as f64).collect());
            let turns = median(rs.iter().map(|r| r.num_turns as f64).collect());
            let cost = median(rs.iter().map(|r| r.cost_usd).collect());
            let secs = median(rs.iter().map(|r| r.duration_ms as f64 / 1e3).collect());
            let ok = rs.iter().filter(|r| r.success).count();
            totals[i].0 += rs.iter().map(|r| r.total_tokens() as f64).sum::<f64>();
            totals[i].1 += rs.iter().map(|r| r.cost_usd).sum::<f64>();
            totals[i].2 += ok;
            totals[i].3 += rs.len();
            let lint_cell = match *variant {
                v if v.starts_with("rewrite") => "(rewrite of vague)".to_string(),
                v => {
                    let prompt = if v == "specific" {
                        &task.specific
                    } else {
                        &task.vague
                    };
                    let codes: Vec<_> = lint(prompt).into_iter().map(|f| f.code).collect();
                    if codes.is_empty() {
                        "clean".to_string()
                    } else {
                        codes.join(", ")
                    }
                }
            };
            let _ = writeln!(
                out,
                "| {} | {variant} | {lint_cell} | {ok}/{} | {:.0}k | {turns:.0} | ${cost:.4} | {secs:.0}s |",
                task.name,
                rs.len(),
                tokens / 1e3,
            );
            medians.push((tokens, cost));
        }
        for (i, variant) in variants.iter().enumerate().skip(1) {
            let _ = writeln!(
                out,
                "| | **Δ {variant} vs vague** | | | **{}** | | **{}** | |",
                delta(medians[0].0, medians[i].0),
                delta(medians[0].1, medians[i].1)
            );
        }
    }

    for split in ["dev", "held-out"] {
        let in_split: Vec<&Run> = runs.iter().filter(|r| r.split == split).collect();
        if in_split.is_empty() {
            continue;
        }
        let _ = writeln!(out, "\n**All {split} runs:**\n");
        let totals: Vec<(f64, f64, usize, usize)> = variants
            .iter()
            .map(|v| {
                let rs: Vec<&&Run> = in_split.iter().filter(|r| r.variant == *v).collect();
                (
                    rs.iter().map(|r| r.total_tokens() as f64).sum(),
                    rs.iter().map(|r| r.cost_usd).sum(),
                    rs.iter().filter(|r| r.success).count(),
                    rs.len(),
                )
            })
            .collect();
        for (variant, (tokens, cost, ok, n)) in variants.iter().zip(&totals) {
            let vs_vague = if *variant == "vague" {
                String::new()
            } else {
                format!(
                    " ({} tokens, {} cost vs vague)",
                    delta(totals[0].0, *tokens),
                    delta(totals[0].1, *cost)
                )
            };
            let _ = writeln!(
                out,
                "- {variant}: {ok}/{n} succeeded, ${cost:.3} total, {:.0}k tokens{vs_vague}",
                tokens / 1e3
            );
        }
    }
    let _ = writeln!(
        out,
        "\nTokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; \
         rewrite variants also include the rewrite call. Cache reads are billed at a fraction of input price, \
         so cost is the better single number."
    );
    out
}

fn delta(base: f64, new: f64) -> String {
    if base == 0.0 {
        return "n/a".into();
    }
    format!("{:+.0}%", (new - base) / base * 100.0)
}

fn claude_version(claude: &str) -> String {
    Command::new(claude)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}
