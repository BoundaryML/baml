# Standard Library Conventions

## Naming

Name casing convention in idiomatic BAML essentially follows Rust:

- Primitive types are lowercase: `int`, `float`, `string`, `uint8array`, etc.
- Declared type positions are PascalCase: classes, enums, interfaces, type aliases, generics, associated types, enum variants, etc.
  - There are a limited set of exceptions: `json` and the media types recieve lowercase aliases.
- Value-positions are snake_case: function names, variables, fields, parameters, etc.

Since names are written fully qualified in BAML, types should avoid stutter when possible: `baml.json.DecodeError` instead of `baml.json.JsonDecodeError`.

## Doc Comments

Every public item should have a human-reviewed (or preferably human-written) doc comment.
They should start with a summary line then a more detailed description.
As needed they should then have markdown H1 headers in order (sections may be omitted if simple or not applicable, e.g. most are only relevant to functions):

````md
# Parameters

- `arg_name`: describing the argument
- etc

# Returns

Describing what it returns

# Throws

- `baml.json.DecodeError` describing cases where it might throw this error
- etc

# Panics

- `baml.panics.IntegerOverflow` describing cases where inputs may cause it to raise this panic
- etc

# Examples

```baml
// usage example
```
````

Doc strings should use the package name (e.g. `baml`) instead of `root` when referring to items.

## Internals

Until such time as we have private items/members, the standard library's internal functions are differentiated only by convention to inform users that they should not be called directly or relied upon.

Internal items and members should be `_`-prefixed and marked with a `/// (internal)` doc comment.
