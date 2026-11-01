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

| set | prompts | precision | recall | F1 | TP | FP | FN | TN |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| dev | 100 | 100% | 92% | 0.96 | 34 | 0 | 3 | 63 |
| held-out | 40 | 100% | 88% | 0.94 | 15 | 0 | 2 | 23 |

| category | dev flagged | held-out flagged |
|---|---:|---:|
| vague | 28 / 30 | 14 / 15 |
| specific | 0 / 30 | 0 / 12 |
| conversational | 0 / 25 | 0 / 8 |
| hard | 6 / 15 | 1 / 5 |

<details><summary>Misclassified, dev (3)</summary>

- missed (vague): `something is off with the API, fix it`
- missed (vague): `pagination is broken, can you fix it`
- missed (hard): `add dark mode`

</details>

<details><summary>Misclassified, held-out (2)</summary>

- missed (vague): `it throws an exception, fix it`
- missed (hard): `make the tests pass`

</details>


## 3. Throughput

| operation | median of 5 | rate |
|---|---:|---:|
| `stats` scan, 101 MB / 85169 lines | 137 ms | 739 MB/s · 0.62M lines/s |
| `lint`, in-process | 1.60 µs/prompt | 624k prompts/s |

## 4. Hook latency

End-to-end cost of `pacenote hook prompt` per submitted prompt (process spawn + parse + lint + output), 200 runs.

| p50 | p95 | p99 | max |
|---:|---:|---:|---:|
| 1.5 ms | 1.7 ms | 2.1 ms | 77.8 ms |

