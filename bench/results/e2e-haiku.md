
# pacenote end-to-end benchmark

Claude Code `2.1.316 (Claude Code)` · model `haiku` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| failing-test | vague | no-done-criteria | 5/5 | 98k | 9 | $0.0037 | 13s |
| failing-test | specific | clean | 5/5 | 55k | 4 | $0.0025 | 8s |
| | **Δ specific vs vague** | | | **-44%** | | **-33%** | |
| production-error | vague | large-paste, no-done-criteria | 5/5 | 247k | 7 | $0.0096 | 18s |
| production-error | specific | clean | 5/5 | 78k | 5 | $0.0034 | 11s |
| | **Δ specific vs vague** | | | **-69%** | | **-65%** | |
| red-ci | vague | vague-target | 5/5 | 188k | 15 | $0.0099 | 19s |
| red-ci | specific | clean | 5/5 | 80k | 6 | $0.0037 | 12s |
| | **Δ specific vs vague** | | | **-58%** | | **-63%** | |

**All runs:** vague 15/15 succeeded, $0.119 total, 2801k tokens · specific 15/15 succeeded, $0.049 total, 1105k tokens (-61% tokens, -59% cost).

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`. Cache reads are billed at a fraction of input price, so cost is the better single number.
