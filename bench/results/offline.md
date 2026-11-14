# pacenote offline benchmark

pacenote 0.1.0 · macos/aarch64 · 8 CPUs · corpus 100 MB

## 1. Stats accuracy

Synthetic transcripts: 50 files, 85169 lines, 20646 API calls written as 1–4 lines each (as Claude Code does), plus malformed and `<synthetic>` lines.

| metric | ground truth | pacenote | naive per-line sum |
|---|---:|---:|---:|
| API calls | 20646 | 20646 (exact) | 51890 (+151.3%) |
| input tokens (all) | 2221093400 | 2221093400 (exact) | 5571544482 (+150.8%) |
| output tokens | 31330964 | 31330964 (exact) | 78652915 (+151.0%) |
| tool results | 12336 | 12336 (exact) | n/a |
| tool result bytes | 78422401 | 78422401 (exact) | n/a |
| malformed lines | 247 | 247 (exact) | n/a |

## 2. Lint quality (`vague-target`)

Everyday engineering prompts, human-labelled by "would Claude have to guess or go searching for what to change?", independent of the rule's implementation. The rule was developed against the **dev** set; the **held-out** set was written before the current rule and never used to tune it.

| language · set | prompts | precision | recall | F1 | TP | FP | FN | TN |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| en · dev | 100 | 100% | 92% | 0.96 | 34 | 0 | 3 | 63 |
| en · held-out | 40 | 100% | 94% | 0.97 | 16 | 0 | 1 | 23 |
| el · dev | 41 | 100% | 100% | 1.00 | 17 | 0 | 0 | 24 |
| el · held-out | 16 | 100% | 100% | 1.00 | 7 | 0 | 0 | 9 |
| fr · dev | 41 | 100% | 100% | 1.00 | 17 | 0 | 0 | 24 |
| fr · held-out | 16 | 100% | 100% | 1.00 | 7 | 0 | 0 | 9 |
| es · dev | 41 | 100% | 100% | 1.00 | 17 | 0 | 0 | 24 |
| es · held-out | 16 | 100% | 100% | 1.00 | 7 | 0 | 0 | 9 |
| de · dev | 41 | 100% | 100% | 1.00 | 17 | 0 | 0 | 24 |
| de · held-out | 16 | 100% | 100% | 1.00 | 7 | 0 | 0 | 9 |

All languages pooled:

| category | dev flagged | held-out flagged |
|---|---:|---:|
| vague | 88 / 90 | 39 / 39 |
| specific | 0 / 78 | 0 / 28 |
| conversational | 0 / 65 | 0 / 24 |
| hard | 14 / 31 | 5 / 13 |

<details><summary>Misclassified, en · dev (3)</summary>

- missed (vague): `something is off with the API, fix it`
- missed (vague): `pagination is broken, can you fix it`
- missed (hard): `add dark mode`

</details>

<details><summary>Misclassified, en · held-out (1)</summary>

- missed (hard): `make the tests pass`

</details>


## 3. Throughput

| operation | median of 5 | rate |
|---|---:|---:|
| `stats` scan, 101 MB / 85169 lines | 134 ms | 756 MB/s · 0.64M lines/s |
| `lint`, in-process | 3.25 µs/prompt | 307k prompts/s |

## 4. Hook latency

End-to-end cost of `pacenote hook prompt` per submitted prompt (process spawn + parse + lint + output), 200 runs.

| p50 | p95 | p99 | max |
|---:|---:|---:|---:|
| 1.4 ms | 1.7 ms | 2.0 ms | 72.3 ms |

