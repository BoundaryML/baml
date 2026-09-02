# JMESPath benchmark specification

Implement this function without changing its signature or public types:

```baml
function jmespath_search(
    expression: string,
    source: JsonSource,
) -> JmesPathResult
    throws baml.errors.Io | baml.errors.Timeout | baml.json.JsonParseError
```

The complete language grammar and semantics are defined by the pinned official
specification in `third_party/jmespath-specification.rst`. That document takes
precedence over examples and other JMESPath implementations.

## Source loading

Resolve `source` before evaluating the expression:

- `InlineJsonSource` uses `value` directly.
- `FileJsonSource` reads `path` as UTF-8 and parses its complete contents as
  JSON.
- `HttpJsonSource` performs an HTTP GET to `url`, passes every supplied header,
  requires a 2xx response, reads the complete response body as UTF-8, and parses
  it as JSON. Use a 30 second total timeout.

Do not infer a source kind from a string. Match exhaustively on `JsonSource`.
File and transport failures throw `baml.errors.Io`, HTTP timeouts throw
`baml.errors.Timeout`, and invalid JSON throws `baml.json.JsonParseError`. Treat
a non-2xx HTTP status as `baml.errors.Io` with a nonempty message.

## Result

Successful evaluation returns `JmesPathSuccess`, including when the JSON value
is `null`.

Expression and evaluation failures return `JmesPathFailure` with a nonempty
message and the exact applicable `JmesPathErrorKind`:

- `Syntax`: invalid JMESPath syntax
- `InvalidArity`: wrong number of arguments to a function
- `InvalidType`: invalid argument type for a function
- `InvalidValue`: a semantically invalid value, including a zero slice step
- `UnknownFunction`: invocation of an unknown function

Do not classify failures by matching strings produced by another error system.
The parser and evaluator must produce the enum directly.

## Scope

Implement the complete language in the pinned specification, including all
built-in functions. Custom functions and implementation-specific extensions are
out of scope.

Object ordering is not significant. Array ordering is significant. JSON numbers
must follow the specification's number semantics within the range supported by
BAML's JSON representation.
