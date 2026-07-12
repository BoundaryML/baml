# `baml describe` agent-navigation benchmark

This directory contains the benchmark evidence that motivated the agent-oriented
`baml describe` changes in this PR. The goal was not to prove that semantic
navigation always beats grep. It was to inspect real agent traces, remove the
specific sources of wasted work, and verify where the redesigned workflow helps
or does not help.

## What changed because of the traces

The first benchmark round showed four recurring problems:

1. Agents started a new CLI process for every symbol they wanted to inspect.
2. The default project map was much larger than the useful answer context.
3. An overview call often became an extra hop before an exact source read.
4. The agent instructions forced `describe` even when a lexical search was the
   better first move.

Those observations led directly to the implementation in this PR:

- `baml describe --grep "term1 || term2"` for bounded project discovery;
- multiple symbols in one invocation;
- `--agent` for compact, citation-oriented output;
- `--limit` and `--budget` for hard output bounds;
- removal of the standalone `baml grep` command so search and description share
  one project-aware surface.

## Round 3 results

Each cell used three repetitions per condition. The transcript files preserve
the exact command sequence and final answer from every run.

### Codex, main trophy-flow question

| condition | median wall time | median calls | median input tokens | median output tokens |
|---|---:|---:|---:|---:|
| natural grep/read | 49s | 3 | 86,726 | 1,692 |
| bounded grep control | 46s | 4 | 78,780 | 1,899 |
| bounded `describe` hybrid | **41s** | 4 | **61,611** | 1,712 |

On the main question, the bounded `describe` workflow was the fastest condition
and used the fewest input tokens. The traces show the intended shape: one
semantic discovery call, one batched description call, then a narrow citation
check when needed.

### Codex, cross-codebase `edit_file` question

| condition | median wall time | median calls | median input tokens | median output tokens |
|---|---:|---:|---:|---:|
| natural grep/read | 47s | 3 | 48,655 | 2,022 |
| bounded grep control | **41s** | 3 | **45,491** | **1,771** |
| bounded `describe` hybrid | 48s | 3 | 50,440 | 1,786 |

The transfer result is deliberately included because it is not a `describe`
win. On this smaller codebase, bounded grep was already cheap. The hybrid still
produced a shorter answer than natural navigation, but did not improve wall time
or input tokens.

### Claude, trophy-flow transfer

| condition | median wall time | median calls | median cost | median judged coverage / precision |
|---|---:|---:|---:|---:|
| natural grep/read | **57s** | 8 | $0.515 | 1.000 / 1.000 |
| bounded `describe` hybrid | 66s | **5** | **$0.325** | 1.000 / 1.000 |

For Claude, the hybrid tied median judged accuracy and reduced both tool calls
and cost, but it was slower. This is useful evidence for the product boundary:
the new surface can make navigation more compact and cheaper without being a
universal latency win across agents and repositories.

## Conclusion

The benchmark supports a narrower, more useful claim than "describe beats
grep": bounded semantic discovery plus batching can materially improve an
agent's navigation loop when a question spans related symbols, while ordinary
grep remains competitive on small or lexically obvious tasks.

That is why this PR keeps normal source reads available and makes `--grep`,
`--agent`, `--limit`, and `--budget` composable instead of forcing one workflow.

## Read the transcripts

- [Codex: trophy-flow matched control](transcripts/codex-trophy-flow.md) — 9
  runs: natural, bounded grep, and bounded `describe` hybrid.
- [Codex: cross-codebase `edit_file` transfer](transcripts/codex-edit-file-transfer.md)
  — 9 runs across the same three conditions.
- [Claude: trophy-flow transfer](transcripts/claude-trophy-flow.md) — 6 runs:
  natural and bounded `describe` hybrid.

These are readable transcript exports containing the question, measurements,
commands, and final answers. Raw JSONL event logs and isolated agent home
directories are intentionally excluded because they contain hidden model event
metadata and credential material that does not belong in a public PR.
