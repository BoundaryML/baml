// Tests for the `baml.FromJson` interface and the `baml.json.to<T>` driver
// (the deserialize counterpart of `baml.ToJson` / `baml.json.from`).
//
// `baml.json.to<T>(j)` decodes a `json` value into a `T`:
//   - if `T` has an `implements baml.FromJson` override, construct via it;
//   - otherwise decode structurally (per-field).
// The auto-derived `from_json` still coexists (F1 is additive), so the
// structural path is still provided by it. These tests pin the decoded result.
//
// `Type.from_json(j)` is static-call sugar for `baml.json.to<Type>(j)`; there
// is no synthesized `from_json` method on the class.
//
// Tests that bind locals (`let`) go through a top-level helper function to
// avoid the test-block local-boxing VM bug; the pure call chain is asserted
// inline.
//
// Different FromJson implementations require unique class names in the shared
// namespace: `Temp` (reads "c") and `TempCelsius` (reads "celsius").
//
// The following tests cannot be expressed as BAML corpus tests because they
// assert on collected compiler errors rather than execution behavior:
//   - from_json_interface_compiles
//   - empty_implementor_is_rejected
//   - static_from_json_charges_only_json_decode_error

// ─── shared declarations ──────────────────────────────────────────────────────

// Implementor whose override reads the `"c"` field rather than a structural
// `celsius` field.
class Temp {
    celsius float
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
        }
    }
}

// Plain class (no FromJson impl) — decodes structurally.
class User { name string  age int }

// ─── to_dispatches_fromjson_override ──────────────────────────────────────────
// `baml.json.to<Temp>(j)` resolves Temp's `baml.FromJson` override, which
// reads the `"c"` field rather than a structural `celsius` field.

function to_dispatches_fromjson_override_fn() -> float throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"c\": 20.5}");
    let t: Temp = baml.json.to<Temp>(j);
    t.celsius
}

test "to_dispatches_fromjson_override" {
    assert.equal(to_dispatches_fromjson_override_fn(), 20.5)
}

// ─── to_decodes_non_implementor_structurally ──────────────────────────────────
// A plain class (no FromJson impl) decodes structurally via `baml.json.to`.

function to_decodes_non_implementor_structurally_fn() -> string throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"name\": \"Ada\", \"age\": 30}");
    let u: User = baml.json.to<User>(j);
    u.name
}

test "to_decodes_non_implementor_structurally" {
    assert.equal(to_decodes_non_implementor_structurally_fn(), "Ada")
}

// ─── to_decodes_primitive ─────────────────────────────────────────────────────

test "to_decodes_primitive" {
    assert.equal(baml.json.to<int>(baml.json.parse("42")), 42)
}

// ─── from_to_roundtrip_with_override ──────────────────────────────────────────
// `json.to<T>(json.from(x))` round-trips, dispatching the FromJson override
// (this implementor's override reads "celsius", matching the structural
// rendering `baml.json.from` produces).

class TempCelsius {
    celsius float
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            TempCelsius { celsius: baml.json.to<float>(baml.json.field(j, "celsius")) }
        }
    }
}

function from_to_roundtrip_with_override_fn() -> bool throws baml.json.SerializationError | baml.json.DecodeError {
    let original: TempCelsius = TempCelsius { celsius: 36.6 };
    let j: baml.json.json = baml.json.from(original);
    let decoded: TempCelsius = baml.json.to<TempCelsius>(j);
    decoded.celsius == 36.6
}

test "from_to_roundtrip_with_override" {
    assert.is_true(from_to_roundtrip_with_override_fn())
}

// ─── nested_field_override_is_honored ─────────────────────────────────────────
// `Outer` has no FromJson impl, so it decodes via the per-field Rust decoder;
// its `Inner` field implements FromJson and must decode via its override
// (which reads "wrapped", not a structural "v").

class Inner {
    v int
    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            Inner { v: baml.json.to<int>(baml.json.field(j, "wrapped")) }
        }
    }
}

class Outer { inner Inner  tag string }

function nested_field_override_is_honored_fn() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"inner\": {\"wrapped\": 7}, \"tag\": \"x\"}");
    let o: Outer = baml.json.to<Outer>(j);
    o.inner.v
}

test "nested_field_override_is_honored" {
    assert.equal(nested_field_override_is_honored_fn(), 7)
}

// ── The `Type.from_json(j)` static-call sugar ─────────────────────────────────
// These exercise the sugar form directly (the tests above use the
// `baml.json.to` driver).

// ─── static_from_json_decodes_non_implementor ─────────────────────────────────
// `User.from_json(j)` (no FromJson impl) desugars to `baml.json.to<User>(j)`
// and decodes structurally.

function static_from_json_decodes_non_implementor_fn() -> string throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"name\": \"Ada\", \"age\": 30}");
    let u: User = User.from_json(j);
    u.name
}

test "static_from_json_decodes_non_implementor" {
    assert.equal(static_from_json_decodes_non_implementor_fn(), "Ada")
}

// ─── static_from_json_dispatches_override ─────────────────────────────────────
// An implementor's `Temp.from_json(j)` resolves Temp's own `baml.FromJson`
// override (not the sugar) — it reads "c", not a structural "celsius".

function static_from_json_dispatches_override_fn() -> float throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"c\": 20.5}");
    let t: Temp = Temp.from_json(j);
    t.celsius
}

test "static_from_json_dispatches_override" {
    assert.equal(static_from_json_dispatches_override_fn(), 20.5)
}

// ─── static_from_json_threads_generic_type_arg ────────────────────────────────
// `Box<int>.from_json(j)` — the `<int>` parses as the call's type args and is
// applied to the receiver, so it desugars to `baml.json.to<Box<int>>(j)` and
// the field decodes as int (the case the `-> Self` delegate could not thread).

class Box<T> { value T }

function static_from_json_threads_generic_type_arg_fn() -> int throws baml.json.ParseError | baml.json.DecodeError {
    let j = baml.json.parse("{\"value\": 42}");
    let b: Box<int> = Box<int>.from_json(j);
    b.value
}

test "static_from_json_threads_generic_type_arg" {
    assert.equal(static_from_json_threads_generic_type_arg_fn(), 42)
}
