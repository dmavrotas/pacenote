
# pacenote end-to-end benchmark

Claude Code `2.1.322 (Claude Code)` · model `haiku` · rewrites by `claude-opus-5-5` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| failing-test | vague | no-done-criteria | 5/5 | 125k | 9 | $0.0086 | 11s |
| failing-test | specific | clean | 5/5 | 57k | 5 | $0.0027 | 6s |
| failing-test | rewritten | (rewrite of vague) | 5/5 | 218k | 11 | $0.0175 | 23s |
| | **Δ specific vs vague** | | | **-54%** | | **-68%** | |
| | **Δ rewritten vs vague** | | | **+74%** | | **+103%** | |
| production-error | vague | large-paste, no-done-criteria | 5/5 | 251k | 8 | $0.0098 | 16s |
| production-error | specific | clean | 5/5 | 82k | 5 | $0.0037 | 10s |
| production-error | rewritten | (rewrite of vague) | 5/5 | 234k | 13 | $0.1944 | 31s |
| | **Δ specific vs vague** | | | **-67%** | | **-62%** | |
| | **Δ rewritten vs vague** | | | **-7%** | | **+1876%** | |
| red-ci | vague | vague-target | 5/5 | 169k | 12 | $0.0055 | 14s |
| red-ci | specific | clean | 5/5 | 84k | 9 | $0.0039 | 9s |
| red-ci | rewritten | (rewrite of vague) | 5/5 | 145k | 13 | $0.0147 | 17s |
| | **Δ specific vs vague** | | | **-50%** | | **-29%** | |
| | **Δ rewritten vs vague** | | | **-14%** | | **+169%** | |

**All runs:**

- vague: 15/15 succeeded, $0.118 total, 2830k tokens
- specific: 15/15 succeeded, $0.053 total, 1201k tokens (-58% tokens, -55% cost vs vague)
- rewritten: 15/15 succeeded, $1.147 total, 3023k tokens (+7% tokens, +874% cost vs vague)

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; the rewritten variant also includes the rewrite call. Cache reads are billed at a fraction of input price, so cost is the better single number.
