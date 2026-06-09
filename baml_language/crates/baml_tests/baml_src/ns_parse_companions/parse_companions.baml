// Tests for parse companion functions ($parse).
//
// Converted from crates/baml_tests/tests/parse_companions.rs — but kept in Rust
// for now (they still pass there). `$parse` is pure parsing, NOT a network call:
// the same source works in a FLAT (non-namespaced) baml file and via the Rust
// `baml_test!` single-source runner.
//
// BUG (blocks converting them here): in a NAMESPACED file (ns_*/), a `$parse`
// companion's return-type `<T>` threading resolves to `Void`, so calling
// `ParsePayload$parse("{}")` fails at runtime with:
//   ExternalOpFailed(BamlLlmStreamCacheNew, Other("Non-parsable type: Void {...}"))
// The identical code in baml_src/<flat>.baml (no ns_ dir) returns
// `Payload { text: null }` correctly. Once the namespaced `$parse` type-arg
// threading is fixed, move these two tests here:
//   - parse_companion_allows_missing_optional_class_field
//   - parse_companion_allows_missing_nullable_alias_field
