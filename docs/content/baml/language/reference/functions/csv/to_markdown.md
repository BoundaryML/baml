---
title: "csv.to_markdown"
description: "Function csv.to_markdown from the generated baml package reference."
---

Renders rows of `T` as a GitHub-style Markdown table for prompt context.

Headers come from `T`'s field names. `|` is escaped and embedded newlines
become spaces; non-finite floats render as `NaN` / `inf` (prompt text is
not meant to round-trip). Beyond `max_rows`, output is truncated with a
final `… (N more rows)` line.

```baml
function csv.to_markdown<T>(rows: T[], max_rows: int) -> string
```

_Source: `<builtin>/baml/ns_csv/csv.baml:28545`_
