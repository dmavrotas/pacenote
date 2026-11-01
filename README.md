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
| `stats` accuracy vs ground truth | exact (a naive per-line sum overcounts by ~150%) |
| `vague-target` on held-out prompts | 100% precision, 88% recall |
| hook overhead per prompt | ~1.5 ms p50 |
| scan throughput | ~740 MB/s |

## Roadmap

- [x] Reproducible benchmarks: offline accuracy/quality/latency + end-to-end token savings

- [x] `pacenote hook install` / `uninstall`: edit settings.json safely
- [ ] Per-session drill-down and "most expensive turns"
- [ ] Context-bloat audit: oversized CLAUDE.md / memory files, MCP tool schema weight
- [ ] `PostToolUse` analysis of repeated reads of the same file
- [ ] Opt-in prompt rewriting (local rules first, optional small model)
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
