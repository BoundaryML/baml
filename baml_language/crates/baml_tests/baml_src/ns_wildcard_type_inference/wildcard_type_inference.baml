// Tests around the `_` wildcard inference hole in type positions (B-230 /
// B-247) — stdlib throws-precision companion tests.

// ─── stdlib throws precision: baml.json.to_json is serialize-only ─────────────
//
// `baml.json.to_json` is a pure serializer: it declares
// `throws baml.json.SerializationError` only (no spurious `baml.json.ParseError`), so a
// caller can declare exactly that error without an exhaustiveness violation.
// The point of the test is that `wildcard_json_dump` COMPILES with the exact
// declared throws; the executed assertion mirrors the Rust `main() -> 0`.

function wildcard_json_dump(x: int) -> baml.json.json throws baml.json.SerializationError {
    baml.json.to_json(x)
}

function wildcard_json_main() -> int { 0 }

test "json_to_json_is_serialize_only" {
    assert.equal(wildcard_json_main(), 0)
}
