# General `baml describe` Claude benchmark

This package contains the broad follow-up benchmark for PR #3989. The measured
questions do not name `baml describe` or a view. They ask ordinary codebase
questions ranging from “What does this function do?” to callers, tests, errors,
change impact, and a cross-file parser feature.

## Setup

- Agent: Claude Code
- Questions: 8 intent types across 3 BAML projects
- Conditions: natural navigation, bounded grep control, direct guided BAML
  navigation, and a dedicated BAML describe subagent
- Repetitions: 1 per cell, 32 final cells
- BAML commit for the original three-condition matrix: `257fdab49`
- BAML commit for the final direct-guided Q4 rerun and subagent matrix:
  `40546bde9`

One repetition per cell makes this directional evidence, not a stable
population estimate.

## Overall result

| condition | median wall | median calls | median cost | median context | median output |
|---|---:|---:|---:|---:|---:|
| natural navigation | **23s** | 3 | $0.203 | 84.7k | 1,481 |
| bounded grep control | 25.5s | 4 | $0.155 | 95.9k | 1,416 |
| guided BAML navigation | 25s | **2** | **$0.117** | 71.0k | **1,005** |
| BAML describe subagent | 30s | 3.5 | $0.129 | **69.9k** | 1,567 |

Direct guidance remained the best overall efficiency and accuracy tradeoff. The
subagent processed slightly less context, but the delegation turn made it five
seconds slower, 75% higher in calls, and 10% more expensive.

Against natural navigation, the subagent cost 37% less and processed 17% less
context, but was seven seconds slower and used more calls.

## Accuracy

Natural, grep, and direct guided navigation each produced seven complete core
answers and one partial answer. The subagent produced five complete and three
partial answers.

- Natural and grep missed part of the propagated typed-error set for
  `agent.tool_edit_file`.
- Direct guided BAML missed the lexical `trophy_spec` prompt contract when
  reasoning about changing `TrophyReport.task_completed` to an enum.
- The subagent also missed `trophy_spec`, treated builtin-thrown errors as out
  of scope for `tool_edit_file`, and stopped before the `parse_primary` error
  origin for `Parser.parse_stmt`.

## Per-question result

| question | result |
|---|---|
| Q1: What does `parse_trophy` do? | Direct guided BAML won at 14s and one call. The subagent was correct at 18s and two calls. |
| Q2: Where is `parse_trophy` used? | Natural won at 14s and one call. The subagent over-expanded to five calls and 29s. |
| Q3: Impact of changing `task_completed` | The subagent was fastest, cheapest, and lowest-context, but repeated guided BAML's partial answer by missing `trophy_spec`. |
| Q4: What does `tool_edit_file` do? | Direct guided BAML won at 17s and one call. The subagent was correct at 28s and two calls. |
| Q5: What errors can `tool_edit_file` return or throw? | The subagent was fastest at 22s but missed propagated builtin errors. Direct guided BAML was the only complete typed-error answer. |
| Q6: Which tests cover `tool_edit_file`? | Direct guided BAML won at 19s and two calls. The subagent eventually answered correctly but needed 11 calls and 67s. |
| Q7: What errors can `Parser.parse_stmt` throw? | The subagent was fastest and cheapest but incomplete because it missed `parse_primary`; bounded grep was the best complete answer. |
| Q8: Add C-style `break;` | The subagent won wall time, cost, and context with a complete answer: 82s, $0.277, 158.4k context. It used the most calls at 16. |

## What delegation actually did

The main Opus agent delegated exactly once in all eight subagent cells and made
zero Bash calls itself. The nested Sonnet navigator used `baml describe` for
every source or relationship lookup.

Simple source questions routed well: Q1, Q4, and Q5 each used one describe call
inside the subagent. Harder intents exposed weak local decisions. Test discovery
expanded to ten nested Bash calls in Q6, while Q8 used fifteen despite the
specialist prompt's batching guidance.

Context totals for the delegated condition include the main Opus agent and the
nested Sonnet agent. The small Haiku classifier is excluded from context totals
but included in total cost, matching the original matrix accounting.

## Files

- [`metrics.jsonl`](metrics.jsonl): one normalized record per final cell
- [`transcripts/`](transcripts/): natural, grep, and direct-guided commands and
  final answers
- [`subagent-transcripts/`](subagent-transcripts/): delegated commands and final
  answers
- [`baml-describe-delegator.md`](baml-describe-delegator.md): measured main-agent
  project instructions
- [`baml-describe-subagent.md`](baml-describe-subagent.md): measured specialist
  prompt
- [`subagent-run-config.txt`](subagent-run-config.txt): frozen model, prompt, and
  accounting provenance

Raw JSONL event logs and isolated Claude homes are excluded because they contain
authentication material and hidden event metadata.

## Reproduce the subagent condition

Run the standalone script against any local BAML project and ordinary codebase
question:

```bash
./docs/benchmarks/baml-describe-multiview/run-claude-subagent.sh \
  ../baml-demos/bamlcode \
  "What kinds of errors can agent.tool_edit_file return or throw?"
```

The runner builds and freezes the release CLI from the current checkout,
creates an isolated target copy and Claude home, materializes the exact measured
delegator and specialist prompts, and exports a readable summary. Do not publish
its raw output directory because it contains the isolated Claude credentials.
