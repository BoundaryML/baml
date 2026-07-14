# `baml describe --view dependencies` benchmark

This package contains the Claude-only follow-up benchmark for the dependency
view added in PR #3989. It tests whether a known-symbol dependency query reduces
navigation work without losing the core contract and implementation dependency
set.

## Result

Three codebases, three conditions, and two repetitions produced six runs per
condition.

| condition | median wall | median calls | median cost | median context |
|---|---:|---:|---:|---:|
| natural navigation | 52.5s | 6.5 | $0.281 | 163.0k |
| bounded grep control | 66s | 7.5 | $0.253 | 209.6k |
| guided `describe` | **37s** | **3.5** | **$0.152** | **95.3k** |

Compared with natural navigation, guided `describe` was 30% faster, used 46%
fewer tool calls, cost 46% less, and processed 42% less context.

## Conditions

- `natural`: no project navigation instructions.
- `guided-grep`: the matched control. Claude receives the same bounded-workflow
  discipline but uses lexical grep/read tools rather than `baml describe`.
- `guided-hybrid-slim`: Claude receives the compact BAML navigation note and can
  combine `baml describe` with narrow source verification.

## Cases

| case | natural | grep control | guided `describe` |
|---|---:|---:|---:|
| `parse_trophy` | 52.5s / 6.5 calls / $0.281 / 171.6k | 67.5s / 9.5 / $0.274 / 229.2k | **31.5s / 3.5 / $0.131 / 95.3k** |
| `agent.tool_edit_file` | 41s / 3 calls / $0.262 / 109.1k | 56s / 4 / $0.196 / 121.9k | **37s / 2.5 / $0.121 / 82.6k** |
| `root.cc.Parser.parse_stmt` | 68s / 9 calls / $0.334 / 215.4k | 68s / 8.5 / $0.287 / 209.6k | **55s / 4.5 / $0.203 / 116.4k** |

Each cell is `wall / calls / cost / context`, using the median of two runs.

## Bug found by the benchmark

The first parser matrix exposed an ambiguous-name resolution bug. The method
referenced `TokKind.Assign`, but the dependency view reported the unrelated AST
class `Assign` because go-to-definition resolved global names before member
positions.

Commit `45d70704e` changed member resolution to prefer the qualified enum
variant, added go-to-definition and dependency regression tests, and produced:

```text
variant root.cc.TokKind.Assign baml_src/ns_cc/lexer.baml:18
```

The six parser runs in this package are the clean rerun after that fix. The
pre-fix parser traces are intentionally excluded from the published result.

## Accuracy note

Manual source review confirmed complete core dependency coverage after the
resolver fix. Some answers in every condition added behavioral explanations
beyond what the source proved. This package therefore supports the
navigation-efficiency claim, not a claim of perfect generated prose precision.

## Transcripts

The transcript exports preserve each run's measurements, command sequence, and
final answer while removing local absolute paths:

- [Claude: `parse_trophy`](transcripts/parse-trophy.md)
- [Claude: `agent.tool_edit_file`](transcripts/tool-edit-file.md)
- [Claude: `root.cc.Parser.parse_stmt`](transcripts/parser-parse-stmt.md)

Raw JSONL event logs and isolated `claude-home` directories are excluded because
they contain hidden model metadata and authentication material.

## Reproduce

The benchmark runner lives in the companion benchmark repository:

<https://github.com/ashley-ha/bamallama/blob/agent-benchmark/scripts/run-describe-dependencies-benchmark.sh>

Run the full matrix:

```bash
./scripts/run-describe-dependencies-benchmark.sh
```

Run a cheaper natural-versus-describe smoke test:

```bash
REPS=1 VARIANTS=natural,guided-hybrid-slim \
  ./scripts/run-describe-dependencies-benchmark.sh
```

The frozen BAML binary for the final parser rerun came from commit `45d70704e`.
The earlier two cases used `e9611c530`; the resolver patch does not affect their
symbols or dependency output.
