
# pacenote end-to-end benchmark

Claude Code `2.1.322 (Claude Code)` · model `haiku` · rewrites by `claude-haiku-5-5` · 5 runs per variant · medians shown

| task | variant | pacenote lint | success | tokens | turns | cost | time |
|---|---|---|---:|---:|---:|---:|---:|
| failing-test | vague | no-done-criteria | 5/5 | 79k | 8 | $0.0035 | 8s |
| failing-test | specific | clean | 5/5 | 57k | 5 | $0.0027 | 6s |
| failing-test | rewrite-local | (rewrite of vague) | 5/5 | 97k | 7 | $0.0035 | 8s |
| failing-test | rewrite-llm | (rewrite of vague) | 5/5 | 59k | 5 | $0.0032 | 9s |
| | **Δ specific vs vague** | | | **-28%** | | **-24%** | |
| | **Δ rewrite-local vs vague** | | | **+22%** | | **-1%** | |
| | **Δ rewrite-llm vs vague** | | | **-25%** | | **-9%** | |
| production-error | vague | large-paste, no-done-criteria | 5/5 | 252k | 7 | $0.0099 | 15s |
| production-error | specific | clean | 5/5 | 79k | 5 | $0.0033 | 8s |
| production-error | rewrite-local | (rewrite of vague) | 5/5 | 125k | 7 | $0.0048 | 13s |
| production-error | rewrite-llm | (rewrite of vague) | 5/5 | 132k | 8 | $0.0095 | 18s |
| | **Δ specific vs vague** | | | **-69%** | | **-67%** | |
| | **Δ rewrite-local vs vague** | | | **-51%** | | **-52%** | |
| | **Δ rewrite-llm vs vague** | | | **-48%** | | **-5%** | |
| red-ci | vague | vague-target | 5/5 | 147k | 13 | $0.0053 | 13s |
| red-ci | specific | clean | 5/5 | 83k | 6 | $0.0038 | 9s |
| red-ci | rewrite-local | (rewrite of vague) | 5/5 | 144k | 13 | $0.0051 | 13s |
| red-ci | rewrite-llm | (rewrite of vague) | 5/5 | 151k | 16 | $0.0061 | 18s |
| | **Δ specific vs vague** | | | **-43%** | | **-29%** | |
| | **Δ rewrite-local vs vague** | | | **-2%** | | **-4%** | |
| | **Δ rewrite-llm vs vague** | | | **+3%** | | **+16%** | |

**All dev runs:**

- vague: 15/15 succeeded, $0.095 total, 2631k tokens
- specific: 15/15 succeeded, $0.049 total, 1061k tokens (-60% tokens, -49% cost vs vague)
- rewrite-local: 15/15 succeeded, $0.067 total, 1800k tokens (-32% tokens, -30% cost vs vague)
- rewrite-llm: 15/15 succeeded, $0.100 total, 1800k tokens (-32% tokens, +5% cost vs vague)

Tokens = input + cache writes + cache reads + output, as reported by `claude -p --output-format json`; rewrite variants also include the rewrite call. Cache reads are billed at a fraction of input price, so cost is the better single number.
