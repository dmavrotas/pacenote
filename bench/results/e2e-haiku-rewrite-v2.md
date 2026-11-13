
# pacenote end-to-end benchmark

Claude Code `2.1.322 (Claude Code)` · model `haiku` · rewrites by `claude-haiku-5-5` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| failing-test | vague | no-done-criteria | 5/5 | 125k | 9 | $0.0085 | 11s |
| failing-test | specific | clean | 5/5 | 57k | 5 | $0.0027 | 6s |
| failing-test | rewritten | (rewrite of vague) | 5/5 | 59k | 5 | $0.0031 | 9s |
| | **Δ specific vs vague** | | | **-54%** | | **-68%** | |
| | **Δ rewritten vs vague** | | | **-53%** | | **-64%** | |
| production-error | vague | large-paste, no-done-criteria | 5/5 | 252k | 8 | $0.0103 | 18s |
| production-error | specific | clean | 5/5 | 82k | 5 | $0.0036 | 9s |
| production-error | rewritten | (rewrite of vague) | 5/5 | 175k | 9 | $0.0109 | 23s |
| | **Δ specific vs vague** | | | **-68%** | | **-65%** | |
| | **Δ rewritten vs vague** | | | **-31%** | | **+6%** | |
| red-ci | vague | vague-target | 5/5 | 121k | 11 | $0.0045 | 12s |
| red-ci | specific | clean | 5/5 | 84k | 9 | $0.0040 | 9s |
| red-ci | rewritten | (rewrite of vague) | 5/5 | 289k | 20 | $0.0124 | 26s |
| | **Δ specific vs vague** | | | **-30%** | | **-11%** | |
| | **Δ rewritten vs vague** | | | **+139%** | | **+175%** | |

**All runs:**

- vague: 15/15 succeeded, $0.114 total, 2668k tokens
- specific: 15/15 succeeded, $0.052 total, 1195k tokens (-55% tokens, -54% cost vs vague)
- rewritten: 15/15 succeeded, $0.130 total, 2534k tokens (-5% tokens, +14% cost vs vague)

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; the rewritten variant also includes the rewrite call. Cache reads are billed at a fraction of input price, so cost is the better single number.
