# baml-bench-experiment — describe-only model benchmark

How well do current OpenAI and Anthropic models write BAML when their only
documentation is `baml describe`?

Each model gets the same task — build a small receipt-extraction BAML project —
and exactly three actions:

- **Describe** one symbol or module (runs `baml describe`),
- **WriteFile** one `.baml` file beneath `baml_src/`,
- **Done**.

No syntax guide, no examples, no compiler, no tests. The model cannot check its
work; it must discover the language through the reference explorer and write
source blind. Turns are capped at 20 per model.

The harness is itself a BAML program, packed into a standalone binary.

## Models

The current API lineup from both providers (pro-tier variants excluded — at
$30/$180 per MTok they don't fit the budget):

| provider  | models |
|-----------|--------|
| openai    | gpt-5.5, gpt-5.4, gpt-5.4-mini, gpt-5.4-nano |
| anthropic | claude-fable-5, claude-opus-4-8, claude-sonnet-5, claude-haiku-4-5 |

## Run

```bash
cd tools/experiments/benchmark-models

# build the harness binary
baml pack Main -o baml-bench-experiment --output-format debug

# builder credentials
export OPENAI_API_KEY=...
export ANTHROPIC_API_KEY=...

# intentionbench grader (must be the local subscription proxy)
export LLM_BASE_URL=http://localhost:19090
export LLM_API_KEY=devproxytoken

./baml-bench-experiment all                     # the full matrix
./baml-bench-experiment gpt-5.5 claude-sonnet-5 # specific models
./baml-bench-experiment anthropic               # one provider's lineup
```

Builds run in parallel (one worker subprocess per model — the binary re-execs
itself so the env-driven builder client binds to exactly one model per
process). Each generated project then gets an independent `baml check`,
`baml test`, and an intentionbench grade, sequentially.

Existing `runs/<model>/project` directories are re-used without a new budget
reservation; delete a run directory to force a rebuild.

## Hard $20 budget

Before each build the harness reserves a deliberately conservative worst case
(8k input + 4k output tokens per turn × 20 turns at published prices) in the
persistent `budget.json` ledger and refuses to start any build that would push
total reservations past $20. The full 8-model matrix reserves ≈ $15.90;
actual spend is far lower. The grader must point at `localhost` so its calls
ride the subscription proxy instead of metered API spend.

## Outputs

```text
runs/<model>/
├── project/                 generated BAML project + builder-transcript.md
├── bench/                   intentionbench report.json / report.md + cache
├── build.json               turns, describes, writes, llm errors, finished
├── build.log  check.log  test.log  grade.log

results.json                 machine-readable comparison rows
budget.json                  worst-case reservation ledger
```

The comparison table reports compile/test status, adherence, a
validity-adjusted score (adherence when compiling, otherwise zero — idiomatic-
looking code that does not compile must not win), commission, omission, and
the verified, unrefuted slop count.

## Harness checks

```bash
baml check   # compile the harness
baml test    # 9 token-free unit tests (registry, budget, paths, report parsing)
```
