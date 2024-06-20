# Flexible Parser

## Purpose

This library exposes an interface:

```rust
pub fn from_str(
    of: &OutputFormatContent,
    target: &FieldType,
    raw_string: &str,
    allow_partials: bool,
) -> Result<BamlValueWithFlags>
```

It provides a guarantee that the schema is able to be flexibly parsed out from the input.

Some scenarios include:

- Finding objects when there is prefixing and post fixed text.
- Parsing in field names with aliases
- Casting to the right type
- Wrapping around arrays when necessary
- Obeying constraints
