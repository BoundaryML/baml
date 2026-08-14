# Experimental GraphQL AST queries

`baml graphql` is an experimental, read-only, one-shot interface for querying a BAML project's source model. It performs no network or LLM calls and does not start a server.

## Command contract

The command discovers the nearest project using the same global `--project <PATH>` and `--directory <PATH>` behavior as the rest of the CLI. Supply a GraphQL document with exactly one of `--query <DOCUMENT>`, `--query-file <PATH>`, or stdin when neither flag is present. Use `--variables <JSON>` for a JSON object of GraphQL variables and `--operation <NAME>` when a document contains multiple operations.

`--schema` prints deterministic GraphQL SDL without loading a project. `--introspect` prints the standard GraphQL introspection response as JSON and also does not require a project. These modes conflict with query input options.

Successful queries and GraphQL request failures use the standard GraphQL JSON response shape on stdout. A project with BAML errors produces a GraphQL error with `extensions.code = "BAML_VALIDATION_FAILED"` and deterministic structured diagnostics in `extensions.diagnostics`. GraphQL parse, validation, variables, project loading, and BAML validation failures return a nonzero exit status. Human-oriented progress, colors, and network activity are disabled for this command so stdout remains machine-readable.

## Schema contract

The schema is a stable GraphQL-facing snapshot, not a serialization of Salsa, CST, AST, HIR, or manifest structs. Renaming or reorganizing compiler internals therefore does not imply a schema change.

The query root exposes `project`, `packages`, `files`, `definitions`, `classes`, `enums`, `functions`, `clients`, `generators`, and `tests`. Collection fields accept practical exact-match filters such as `name`, `kind`, or `path`; they deliberately do not implement a general text index. Results use deterministic source order with path and name tie-breakers.

Definitions expose names, qualified names, documentation where present, attributes where present, and source locations. Classes traverse to fields, enums to values, functions to parameters and return or throws types, tests to targeted functions, packages and files to their contained definitions, and every typed declaration traverses a recursive `TypeRef`. `TypeRef` includes a stable kind, source spelling, named path, nested element/key/value/member types as applicable, attributes, and location. Source locations use project-relative slash-separated paths plus 1-based start and end line and column values.

The initial schema covers user-authored classes, fields, enums, enum values, type aliases, functions, parameters and return types, clients, manifest generators, legacy declarative tests where the compiler exposes them, documentation, raw attributes, packages, namespaces, files, and locations. Compiler-generated definitions and builtin package sources are excluded from project results.

## Examples

```sh
baml graphql --query '{ classes(name: "Resume") { name fields { name type { display kind } } } }'
```

```sh
baml graphql --query 'query Find($name: String!) { functions(name: $name) { qualifiedName parameters { name type { display } } returnType { display } location { path startLine startColumn } } }' --variables '{"name":"ExtractResume"}'
```

```sh
baml graphql --query-file ./queries/project.graphql --operation ProjectSummary
```

```sh
printf '%s\n' '{ definitions(kind: [CLASS, ENUM]) { kind name qualifiedName location { path startLine } } }' | baml graphql
```

```sh
baml graphql --schema
baml graphql --introspect
```
