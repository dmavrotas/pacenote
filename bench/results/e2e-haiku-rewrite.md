
# pacenote end-to-end benchmark

Claude Code `2.1.322 (Claude Code)` · model `haiku` · rewrites by `claude-haiku-5-5` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| failing-test | vague | no-done-criteria | 5/5 | 125k | 9 | $0.0048 | 10s |
| failing-test | specific | clean | 5/5 | 58k | 5 | $0.0028 | 6s |
| failing-test | rewritten | (rewrite of vague) | 5/5 | 79k | 5 | $0.0034 | 10s |
| | **Δ specific vs vague** | | | **-54%** | | **-42%** | |
| | **Δ rewritten vs vague** | | | **-37%** | | **-29%** | |
| production-error | vague | large-paste, no-done-criteria | 5/5 | 254k | 7 | $0.0101 | 16s |
| production-error | specific | clean | 5/5 | 81k | 6 | $0.0037 | 11s |
| production-error | rewritten | (rewrite of vague) | 5/5 | 170k | 12 | $0.0109 | 22s |
| | **Δ specific vs vague** | | | **-68%** | | **-63%** | |
| | **Δ rewritten vs vague** | | | **-33%** | | **+8%** | |
| red-ci | vague | vague-target | 5/5 | 122k | 11 | $0.0047 | 13s |
| red-ci | specific | clean | 5/5 | 83k | 6 | $0.0038 | 9s |
| red-ci | rewritten | (rewrite of vague) | 5/5 | 203k | 20 | $0.0116 | 21s |
| | **Δ specific vs vague** | | | **-32%** | | **-20%** | |
| | **Δ rewritten vs vague** | | | **+67%** | | **+147%** | |

**All runs:**

- vague: 15/15 succeeded, $0.110 total, 2553k tokens
- specific: 15/15 succeeded, $0.052 total, 1186k tokens (-54% tokens, -52% cost vs vague)
- rewritten: 15/15 succeeded, $0.121 total, 2270k tokens (-11% tokens, +10% cost vs vague)

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; the rewritten variant also includes the rewrite call. Cache reads are billed at a fraction of input price, so cost is the better single number.
