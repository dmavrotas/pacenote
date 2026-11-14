# pacenote

[![CI](https://github.com/dmavrotas/pacenote/actions/workflows/ci.yml/badge.svg)](https://github.com/dmavrotas/pacenote/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

> A rally co-driver reads pace notes: short, precise instructions so the driver never wastes a corner.
> pacenote does the same for Claude Code: spend fewer tokens, waste fewer turns.

pacenote is a single, fast Rust binary that works alongside [Claude Code](https://claude.com/claude-code):

- **`pacenote stats`** reads your local session transcripts and shows where your tokens actually go: by model, by project, and which tool outputs flood the context window.
- **`pacenote hook prompt`** is a `UserPromptSubmit` hook that warns you, before Claude starts, when a prompt is vague ("fix it") or a paste is oversized. It **adds zero tokens**: feedback goes to you via `systemMessage`, not into Claude's context.
- **`pacenote lint`** runs the same checks on any prompt from the command line.

Everything runs locally. pacenote makes no network calls.

## Install

Prebuilt binaries for every platform Claude Code runs on are attached to each [release](https://github.com/dmavrotas/pacenote/releases):

| OS | x86_64 | arm64 |
|---|---|---|
| macOS | `x86_64-apple-darwin` | `aarch64-apple-darwin` |
| Linux (glibc) | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` |
| Linux (static, Alpine) | `x86_64-unknown-linux-musl` | `aarch64-unknown-linux-musl` |
| Windows | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` |

Each archive ships with a `.sha256` checksum. Or build from source:

```sh
cargo install --git https://github.com/dmavrotas/pacenote
```

## Usage

```sh
pacenote stats                       # all projects
pacenote stats -p myrepo --since 2026-10-01
pacenote stats --json | jq .tools    # machine-readable
pacenote lint "fix the bug"
```

Example:

```
TOTALS
  input (uncached)        1.7k
  cache writes            3.4M
  cache reads           416.7M
  output                  1.4M
  cache hit rate         99.2%

TOOL OUTPUT ENTERING CONTEXT (top 6, est. tokens)
  tool                               calls    tokens      avg      max
  Bash: cat                            132     40.1k      304     6.2k
  Bash: sed                             59     27.8k      470     3.1k
  Bash: git status                      12      6.8k      563     2.4k
```

### Enable the prompt hook

```sh
pacenote hook install                  # ~/.claude/settings.json (all projects)
pacenote hook install --scope project  # .claude/settings.json (shared with your team)
pacenote hook install --scope local    # .claude/settings.local.json (just you)
pacenote hook install --strict         # also report informational findings
pacenote hook install --dry-run        # print the result, write nothing
pacenote hook uninstall
```

The installer is idempotent and only touches pacenote's own entry. It keeps your other hooks and settings (key order included), refuses to edit a file that isn't valid JSON, writes atomically, and saves the previous file as `settings.json.pacenote.bak`.

<details>
<summary>Manual setup</summary>

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "pacenote hook prompt" }] }
    ]
  }
}
```
</details>

## Prompt rewriting (opt-in)

`pacenote rewrite` makes a prompt cheaper to act on. By default it runs **locally**: it's free and deterministic, and nothing leaves your machine. It does only two things, and only when the evidence is clear:

- **Trims pastes.** A pasted log becomes its first error block and stack trace.
- **Names the likely file.** When one source file clearly matches the prompt's words (IDF-weighted, file names beat contents, test files excluded, and the file must win by 1.5×), it appends `Likely location: <file>`.

Otherwise it leaves the prompt alone. "The tests are failing" is already an efficient instruction, so it exits with code 3 ("nothing to add").

```sh
$ pacenote rewrite "checkout is failing in production, here are the logs: <35 KB of logs> fix it"
checkout is failing in production, here are the logs:

First error from the pasted log (407 lines; the rest omitted):
  2026-10-30T14:03:37.412Z ERROR request_id=req-5f3a2c method=POST path=/api/checkout status=500
  Traceback (most recent call last): …
  ValueError: total cannot be negative

fix it
(local · trimmed 42714 bytes of paste)
```

**Measured** (end-to-end benchmark, held-out tasks never used for tuning, 5 runs each, every run fixed its bug):

| vs the vague prompt | tokens | cost |
|---|---:|---:|
| `--mode local` (default) | **−17%** | **−24%** |
| `--mode llm` (Haiku, experimental) | −11% | +1% |
| a person writing the specific prompt pacenote's lint asks for | −51% | −45% |

Most of the local saving comes from paste trimming: −54% cost on a pasted crash log. The file hint helped on 2 of 3 held-out tasks where it applied and was neutral on the third. `--mode llm` uses your own Claude access (`ANTHROPIC_API_KEY`, else your Claude Code login), so a subscription works without an API key. It hasn't beaten local mode yet, so it stays experimental. Details: [bench/results](bench/results/e2e-grounding-holdout.md).

To have the hook do it, run `pacenote hook install --rewrite`. Then:

- **Pasted logs are held.** A prompt flagged for a large paste is held, and the trimmed version is shown, in the prompt's language. Paste it (edit it if needed), or resubmit your original unchanged to send it as-is.
- **File hints are silent.** When one source file clearly matches a task request, the prompt goes through as typed, and Claude gets one line, `Likely location: <file>` (~10 tokens, only when confident). You see a short `pacenote [hint]` note.
- **Otherwise nothing happens.** Replies and questions never get hints. With hints on, the hook adds ~2–3 ms to a reply and ~10–30 ms to a task prompt in a typical repo.

| Option | Default | |
|---|---|---|
| `--mode` | `local` | `local` (free, offline) or `llm` (your Claude access) |
| `--model` | `$PACENOTE_MODEL` or `claude-haiku-5-5` | the cheapest current model; a rewrite costs ~$0.0003 |
| `--backend` (llm) | `auto` | `api` (needs `ANTHROPIC_API_KEY`) or `claude` (your Claude Code login) |
| `--no-context` (llm) | off | By default the rewriter sees the repo name, `git ls-files` and `git status` (capped at 12 KB) so it can name real files. File *contents* are never sent. |
| `--timeout` | 25 s | |

**Privacy:** local mode sends nothing anywhere. `--mode llm` is the only pacenote feature that sends data: it sends the prompt (and, unless `--no-context`, the file list) to Anthropic under your own account, and only when you choose it.

## Languages

Prompts are linted in **English, Greek, French, Spanish and German**, and findings are reported in the prompt's language. Mixed prompts ("διόρθωσε το bug στο `auth.rs`") work, because English technical vocabulary is always recognised alongside the detected language.

```
$ pacenote lint "διόρθωσέ το"
pacenote [vague-target] Δεν υπάρχει συγκεκριμένος στόχος. Ανάφερε το αρχείο, τη συνάρτηση ή το ακριβές μήνυμα σφάλματος, …
```

Each language has a word pack in [`src/lang.rs`](src/lang.rs) and labelled prompts in `bench/prompts.<lang>.jsonl`. Adding a language means adding both; the benchmark scores it automatically.

## Lint rules

| Code | Severity | Fires when |
|---|---|---|
| `vague-target` | warn | A short task request ("fix it", "fix the bug") names no file, identifier, line, or quoted error |
| `large-paste` | warn | The prompt is over ~5k tokens; point to a file or trim to the first error instead |
| `no-done-criteria` | info | A task request doesn't say what success looks like |

Rules only fire on prompts that look like task requests, so replies like "yes" or "go ahead" are never flagged.

## How the numbers work

- Transcripts live in `~/.claude/projects/<project>/<session>.jsonl` (`$CLAUDE_CONFIG_DIR` is respected). Subagent transcripts are included.
- One API response spans several transcript lines that repeat the same usage; pacenote deduplicates by `requestId`.
- Tool-output tokens are **estimates** (bytes ÷ 4) meant for ranking. They are not separate from input tokens: a tool result is written to the cache once and then re-read on every later turn, which is why trimming noisy output pays off for the rest of the session.
- The transcript format is undocumented and may change; unparseable lines are skipped and counted.

## Benchmarks

Reproducible by anyone. See [bench/README.md](bench/README.md) for the methodology.

```sh
cargo bench --bench offline                               # free, deterministic
cargo run --release --example e2e -- --yes                # real Claude Code runs (~$0.15 on Haiku)
```

Latest results: [offline](bench/results/offline.md) · [end-to-end](bench/results/e2e-haiku.md)

**End-to-end** (Claude Code + Haiku, 5 runs per variant, success checked by the fixture's unit tests): following pacenote's advice cut tokens by **44–69%** and cost by **33–65%** per task, and every run in both variants fixed the bug (30/30).

| task | vague prompt | pacenote flags | Δ tokens | Δ cost |
|---|---|---|---:|---:|
| failing test | "pagination is broken, can you fix it" | `no-done-criteria` (info) | −44% | −33% |
| production error | "checkout is failing… ⟨35 KB log⟩ fix it" | `large-paste` | −69% | −65% |
| red CI | "the tests are failing, fix them" | `vague-target` | −58% | −63% |

| | Result |
|---|---|
| `pacenote rewrite` (local, held-out tasks) vs vague prompts | −17% tokens, −24% cost |
| `stats` accuracy vs ground truth | exact (a naive per-line sum overcounts by ~150%) |
| `vague-target` on held-out prompts (5 languages) | 100% precision, 94–100% recall |
| hook overhead per prompt | ~1.5 ms p50 |
| scan throughput | ~740 MB/s |

## Roadmap

- [x] Reproducible benchmarks: offline accuracy/quality/latency + end-to-end token savings

- [x] `pacenote hook install` / `uninstall`: edit settings.json safely
- [ ] Per-session drill-down and "most expensive turns"
- [ ] Context-bloat audit: oversized CLAUDE.md / memory files, MCP tool schema weight
- [ ] `PostToolUse` analysis of repeated reads of the same file
- [x] Opt-in prompt rewriting with your own Claude access (API key or Claude Code login), experimental
- [x] Grounded local rewriting: trim pastes, name the likely file, otherwise leave the prompt alone
- [ ] Parallel scanning for very large transcript directories

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## License

pacenote is free and open source, dual-licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option. This is the same licensing as Rust itself and most of the Rust ecosystem.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in pacenote by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.
