# Task: text-stats

Build a small program that, given a path to a UTF-8 text file, prints a
single-line JSON object to stdout with these four integer fields:

- `bytes`  — file size in bytes (raw byte count of the file).
- `chars`  — Unicode codepoint count (NOT byte count, NOT grapheme count).
- `words`  — number of whitespace-delimited tokens, equivalent to:
  - Python: `len(text.split())`
  - Go: `len(strings.Fields(text))`
- `lines`  — number of `\n` characters in the file (`wc -l` semantics).
  An empty file has 0 lines; `"abc"` (no trailing newline) also has 0
  lines; `"abc\n"` has 1 line; `"a\nb\n"` has 2 lines.

## Invocation contract

Save your implementation at the **exact** path expected for your
language, in the working directory:

| Language | Filename            | Invocation                            |
|----------|---------------------|---------------------------------------|
| Python   | `text_stats.py`     | `python3 text_stats.py <input-file>`  |
| Go       | `text_stats.go`     | `go run text_stats.go <input-file>` (the grader builds it) |
| BAML     | `text_stats.baml`   | (BAML driver TBD; v1 deferral)        |

The grader looks for the candidate at this exact path. Don't pick a
different filename.

Output: a single line of compact JSON, e.g.:

```
{"bytes":12,"chars":12,"words":2,"lines":1}
```

Field order is not significant. Trailing whitespace / final newline is
tolerated; intermediate whitespace inside the JSON is not (use compact
JSON, no spaces).

## Constraints

- Read the file as bytes, then decode as UTF-8.
- Treat the file as binary-safe up to the UTF-8 decode step.
- No third-party dependencies. Use the language standard library only.

## Test coverage

The graders run your program against four input files and assert the
JSON matches expected values. Make all of them pass.

## Banned shortcuts

- Don't shell out to `wc` or another external tool.
- Don't import a third-party JSON library when the stdlib has one.

## Turn / cost budget

- ≤ 20 turns
- ≤ $5 estimated_cost_usd

If you exceed either, the run is marked failed for accounting purposes
even if tests pass.
