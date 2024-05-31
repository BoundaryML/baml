# Flexible Parser

## Purpose

This library exposes an interface:

```rust
fn parse(input: &str, schema: JSONSchema) -> Result<serde_json::Value, DeserializationError>
```

It provides a guarantee that the schema is able to be flexibly parsed out from the input.

Some scenarios include:

- Finding objects when there is prefixing and post fixed text.
- Parsing in field names with aliases
- Casting to the right type
- Wrapping around arrays when necessary
- Obeying constraints
