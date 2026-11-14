
# pacenote end-to-end benchmark

Claude Code `2.1.322 (Claude Code)` · model `haiku` · rewrites by `claude-haiku-5-5` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| stock-report | vague | no-done-criteria | 5/5 | 99k | 8 | $0.0039 | 10s |
| stock-report | specific | clean | 5/5 | 77k | 5 | $0.0030 | 7s |
| stock-report | rewrite-local | (rewrite of vague) | 5/5 | 77k | 6 | $0.0032 | 8s |
| stock-report | rewrite-llm | (rewrite of vague) | 5/5 | 125k | 10 | $0.0049 | 16s |
| | **Δ specific vs vague** | | | **-23%** | | **-24%** | |
| | **Δ rewrite-local vs vague** | | | **-22%** | | **-16%** | |
| | **Δ rewrite-llm vs vague** | | | **+26%** | | **+28%** | |
| signup-email | vague | no-done-criteria | 5/5 | 124k | 11 | $0.0050 | 16s |
| signup-email | specific | clean | 5/5 | 57k | 5 | $0.0027 | 6s |
| signup-email | rewrite-local | (rewrite of vague) | 5/5 | 100k | 7 | $0.0041 | 12s |
| signup-email | rewrite-llm | (rewrite of vague) | 5/5 | 126k | 9 | $0.0050 | 16s |
| | **Δ specific vs vague** | | | **-54%** | | **-46%** | |
| | **Δ rewrite-local vs vague** | | | **-20%** | | **-19%** | |
| | **Δ rewrite-llm vs vague** | | | **+2%** | | **-0%** | |
| top-products | vague | no-done-criteria | 5/5 | 78k | 8 | $0.0034 | 8s |
| top-products | specific | clean | 5/5 | 59k | 4 | $0.0029 | 6s |
| top-products | rewrite-local | (rewrite of vague) | 5/5 | 98k | 6 | $0.0035 | 9s |
| top-products | rewrite-llm | (rewrite of vague) | 5/5 | 102k | 9 | $0.0043 | 14s |
| | **Δ specific vs vague** | | | **-25%** | | **-16%** | |
| | **Δ rewrite-local vs vague** | | | **+25%** | | **+2%** | |
| | **Δ rewrite-llm vs vague** | | | **+30%** | | **+27%** | |
| red-ci-2 | vague | vague-target | 5/5 | 166k | 17 | $0.0068 | 20s |
| red-ci-2 | specific | clean | 5/5 | 80k | 6 | $0.0035 | 9s |
| red-ci-2 | rewrite-local | (rewrite of vague) | 5/5 | 177k | 14 | $0.0062 | 17s |
| red-ci-2 | rewrite-llm | (rewrite of vague) | 5/5 | 195k | 13 | $0.0068 | 21s |
| | **Δ specific vs vague** | | | **-52%** | | **-49%** | |
| | **Δ rewrite-local vs vague** | | | **+7%** | | **-9%** | |
| | **Δ rewrite-llm vs vague** | | | **+17%** | | **+0%** | |
| profile-crash | vague | large-paste, no-done-criteria | 5/5 | 251k | 7 | $0.0101 | 17s |
| profile-crash | specific | clean | 5/5 | 79k | 6 | $0.0033 | 9s |
| profile-crash | rewrite-local | (rewrite of vague) | 5/5 | 123k | 7 | $0.0046 | 14s |
| profile-crash | rewrite-llm | (rewrite of vague) | 5/5 | 103k | 6 | $0.0083 | 13s |
| | **Δ specific vs vague** | | | **-68%** | | **-67%** | |
| | **Δ rewrite-local vs vague** | | | **-51%** | | **-54%** | |
| | **Δ rewrite-llm vs vague** | | | **-59%** | | **-17%** | |

**All held-out runs:**

- vague: 25/25 succeeded, $0.143 total, 3638k tokens
- specific: 25/25 succeeded, $0.078 total, 1792k tokens (-51% tokens, -45% cost vs vague)
- rewrite-local: 25/25 succeeded, $0.108 total, 3013k tokens (-17% tokens, -24% cost vs vague)
- rewrite-llm: 25/25 succeeded, $0.145 total, 3250k tokens (-11% tokens, +1% cost vs vague)

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; rewrite variants also include the rewrite call. Cache reads are billed at a fraction of input price, so cost is the better single number.
