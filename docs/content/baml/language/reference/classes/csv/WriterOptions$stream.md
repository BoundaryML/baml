---
title: "csv.WriterOptions$stream"
description: "Class csv.WriterOptions$stream from the generated baml package reference."
---

Options for CSV writers. All fields are optional; `null` means default.

```baml
class csv.WriterOptions$stream
```

## Fields

### delimiter

```baml
delimiter: string | null
```

Field separator. Exactly one ASCII byte. Default `","`.

### quote

```baml
quote: string | null
```

Quote character. Exactly one ASCII byte. Default `"\""`.

### quote_style

```baml
quote_style: "minimal" | "all" | "never" | null
```

`"minimal"` (default): quote only fields containing the delimiter,
quote, CR, or LF. `"all"`: quote every field. `"never"`: quote nothing;
a field that would require quoting throws `Encode`.

### escape

```baml
escape: string | null
```

`null` (default) means `""` doubling inside quoted fields.

### terminator

```baml
terminator: "lf" | "crlf" | null
```

Record terminator. Default `"lf"`; `"crlf"` for strict RFC 4180 / Excel.

### write_header

```baml
write_header: bool | null
```

Automatically emit a header before the first `write_row<T>`.
Default `true`.

### headers

```baml
headers: string[] | null
```

Override header names for typed writes (positional).

### null_value

```baml
null_value: string | null
```

How `null` serializes. Default `""`.

### bom

```baml
bom: bool | null
```

Emit a UTF-8 BOM (Excel's UTF-8 detection). Default `false`.

### sanitize_formulas

```baml
sanitize_formulas: bool | null
```

CSV-injection guard: prefix `'` to cells starting with `=`, `+`, `-`,
`@`, tab, CR, or LF (including full-width variants). Mutates the data,
which is why it is opt-in. Default `false`.

_Source: `<builtin>/baml/ns_csv/csv.baml:0`_
