# Benchmarks

Two suites back up pacenote's claims. Both can be run by anyone.

## Offline (free, deterministic)

```sh
cargo bench --bench offline                       # ~100 MB synthetic corpus
PACENOTE_BENCH_MB=500 cargo bench --bench offline
```

The report prints to stdout and is saved to `target/bench/offline.md`. CI runs it on every PR and posts the report in the job summary.

| Section | What it proves |
|---|---|
| **Stats accuracy** | Synthetic transcripts with known ground truth: each API call is written as 1–4 lines (as Claude Code does), mixed with malformed and `<synthetic>` lines. pacenote's totals must match exactly, and the bench **fails** otherwise. A naive per-line sum is shown for comparison. |
| **Lint quality** | Precision/recall of `vague-target` on everyday engineering prompts labelled by hand, per language (en, el, fr, es, de): `prompts[.<lang>].jsonl` (dev sets, used for tuning) and `prompts-holdout[.<lang>].jsonl` (written before the rules, never used to tune them). Misclassified prompts are listed, not hidden. |
| **Throughput** | `stats` scan speed and in-process `lint` cost. |
| **Hook latency** | The delay `pacenote hook prompt` adds to every prompt you submit (process spawn included). |

## End-to-end (spends real tokens)

```sh
cargo run --release --example e2e -- --yes                     # 3 tasks × 2 variants × 3 runs, haiku
cargo run --release --example e2e -- --yes --model sonnet --runs 5 --json results.json
```

Requires `claude` (logged in) and `python3`. With Haiku, a full run costs roughly $0.10–0.20.

Each task gives Claude Code (`claude -p`) the same job twice, each time in a fresh copy of [`fixture/`](fixture), a small Python shop with two planted bugs:

| Task | Vague prompt (as typically typed) | Specific prompt (after pacenote's advice) |
|---|---|---|
| `failing-test` | "pagination is broken, can you fix it" | names `paginate()`, the symptom, and the test command |
| `production-error` | "checkout is failing in production, here are the logs: ⟨35 KB log⟩ fix it" | quotes the error, points at `logs/app.log`, names the test |
| `red-ci` | "the tests are failing, fix them" | names the failing test files and says "fix shop/, not the tests" |

The specific prompts say *where* and *what done looks like*. They never say *how* to fix the bug.

**Scoring.** After each run, the fixture's unit tests decide success. A cheap run that didn't fix the bug counts as a failure, not a saving. Token counts and cost come from `claude -p --output-format json`.

**Isolation.** Runs load no user hooks, plugins or MCP servers (`--setting-sources project`, `--strict-mcp-config`), so your local setup (including pacenote's own hook) doesn't affect results. Your user-level `CLAUDE.md` is still loaded, identically for both variants; use an account without one for the cleanest numbers.

**Caveats.** LLM runs are non-deterministic, so use `--runs 5` or more and compare medians. The fixture is small; on real codebases, exploration costs from vague prompts are typically larger.

## Contributing prompts

The lint corpus improves with independent labels, especially from **native speakers**: the non-English corpora and word packs were written by a non-native author. Add everyday prompts you actually send, labelled by whether Claude would have to guess or search for the target, to `prompts-holdout.jsonl` in a PR, ideally without first checking what pacenote says about them.
