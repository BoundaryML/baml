// Tests for the `baml.ToJson` interface and the `baml.json.from<T>` driver.
//
// `baml.json.from(value)` resolves `to_json` on `value`'s *runtime* class:
//   - if that class has an `implements baml.ToJson` override, dispatch to it;
//   - otherwise render structurally via `baml._to_json_default`, which is also
//     the interface's default `to_json` body.
// Each test pins the serialized text (`baml.json.stringify(baml.json.from(v))`)
// so the json shape is asserted exactly.
//
// Tests that bind locals (`let`) go through a top-level helper function to
// avoid the test-block local-boxing VM bug; pure call chains are asserted
// inline. String results are compared with `assert.equal`.

// ─── shared declarations ──────────────────────────────────────────────────────

class User { name string  age int }

// Implementor with a custom override.
class Point {
    x int
    y int
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            { "pt": [baml.json.from(self.x), baml.json.from(self.y)] }
        }
    }
}

// ─── operator_style_to_json_desugars_to_from ──────────────────────────────────
// `obj.to_json()` on a non-implementor has no real method; it desugars to
// `baml.json.from(obj)` (structural), the json analog of `obj.to_string()`.

function desugar_via_from_str() -> string throws baml.json.JsonSerializationError {
    baml.json.stringify(baml.json.from(User { name: "Ada", age: 30 }))
}

function desugar_via_method_str() -> string throws baml.json.JsonSerializationError {
    let u = User { name: "Ada", age: 30 };
    baml.json.stringify(u.to_json())
}

test "operator_style_to_json_desugars_to_from" {
    assert.equal(desugar_via_method_str(), desugar_via_from_str());
    assert.equal(desugar_via_method_str(), "{\"name\":\"Ada\",\"age\":30}")
}

// ─── operator_style_to_json_dispatches_override ───────────────────────────────
// `obj.to_json()` on an implementor resolves the real interface method.

function point_via_method_str() -> string throws baml.json.JsonSerializationError {
    let p = Point { x: 1, y: 2 };
    baml.json.stringify(p.to_json())
}

test "operator_style_to_json_dispatches_override" {
    assert.equal(point_via_method_str(), "{\"pt\":[1,2]}")
}

// ─── default_renders_primitives ───────────────────────────────────────────────

test "default_renders_primitives" {
    assert.equal(baml.json.stringify(baml.json.from(42)), "42");
    assert.equal(baml.json.stringify(baml.json.from(true)), "true");
    assert.equal(baml.json.stringify(baml.json.from(null)), "null");
    assert.equal(baml.json.stringify(baml.json.from("hi")), "\"hi\"");
    assert.equal(baml.json.stringify(baml.json.from(3.5)), "3.5")
}

// ─── default_renders_containers ───────────────────────────────────────────────

test "default_renders_containers" {
    assert.equal(baml.json.stringify(baml.json.from([1, 2, 3])), "[1,2,3]");
    assert.equal(baml.json.stringify(baml.json.from({"a": 1, "b": 2})), "{\"a\":1,\"b\":2}")
}

// ─── default_renders_class_instance_structurally ──────────────────────────────
// A class that does NOT implement ToJson renders as a field map.

test "default_renders_class_instance_structurally" {
    assert.equal(
        baml.json.stringify(baml.json.from(User { name: "Ada", age: 30 })),
        "{\"name\":\"Ada\",\"age\":30}"
    )
}

// ─── empty_implementor_matches_nonimplementor ─────────────────────────────────
// `implements baml.ToJson {}` with no override uses the interface's default
// body — the same structural rendering a non-implementor gets.

class PointEmptyImpl {
    x int
    y int
    implements baml.ToJson {}
}

class PointPlain { x int  y int }

function point_empty_impl_str() -> string throws baml.json.JsonSerializationError {
    baml.json.stringify(baml.json.from(PointEmptyImpl { x: 1, y: 2 }))
}

function point_plain_str() -> string throws baml.json.JsonSerializationError {
    baml.json.stringify(baml.json.from(PointPlain { x: 1, y: 2 }))
}

test "empty_implementor_matches_nonimplementor" {
    assert.equal(point_empty_impl_str(), point_plain_str());
    assert.equal(point_empty_impl_str(), "{\"x\":1,\"y\":2}")
}

// ─── implementor_override_is_used ─────────────────────────────────────────────
// A class implementing ToJson with a custom body: `baml.json.from` resolves
// `to_json` on the value's runtime class and dispatches to the override.

test "implementor_override_is_used" {
    assert.equal(
        baml.json.stringify(baml.json.from(Point { x: 1, y: 2 })),
        "{\"pt\":[1,2]}"
    )
}

// ─── nested_override_is_honored_at_depth ──────────────────────────────────────
// An override-bearing instance nested inside a structurally-rendered class
// must render via its override, not structurally.

class Inner {
    v int
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            { "wrapped": baml.json.from(self.v) }
        }
    }
}

class Outer { inner Inner  tag string }

test "nested_override_is_honored_at_depth" {
    assert.equal(
        baml.json.stringify(baml.json.from(Outer { inner: Inner { v: 7 }, tag: "x" })),
        "{\"inner\":{\"wrapped\":7},\"tag\":\"x\"}"
    )
}

// ─── override_inside_container_is_honored ─────────────────────────────────────
// Override-bearing instances inside a list each render via their override.

class Tag {
    name string
    implements baml.ToJson {
        function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
            baml.json.from(self.name)
        }
    }
}

test "override_inside_container_is_honored" {
    assert.equal(
        baml.json.stringify(baml.json.from([Tag { name: "a" }, Tag { name: "b" }])),
        "[\"a\",\"b\"]"
    )
}

// ─── enum_renders_as_variant_name ─────────────────────────────────────────────

enum Color { Red  Green  Blue }

test "enum_renders_as_variant_name" {
    assert.equal(baml.json.stringify(baml.json.from(Color.Green)), "\"Green\"")
}

// ─── enum & string-literal-union fields in the value→JSON dispatch ────────────
//
// A class field of `enum` type or a string-literal union (`"X" | "Y"`) must
// serialize to a plain JSON string — the variant name / the string — through
// both the structural `baml.json.from` / `.to_json()` path and the typed
// `baml.json.to_string<T>` path. The regression these lock in: a field whose
// declared type is a *union containing an enum* (including the optional enum
// `E?` == `E | null`) serialized as `null` through the typed walker, whose
// `RuntimeTy::Union` arm delegates to the untyped `value_to_serde`, which
// mapped enum variants to `null` instead of the variant-name string.

enum E { A  B }

class C { e E }

test "enum_field_serializes_as_variant_name" {
    // Primary ticket case: `class C { e E }` round-trips through the
    // structural `baml.json.from` path.
    assert.equal(
        baml.json.stringify(baml.json.from(C { e: E.A })),
        "{\"e\":\"A\"}"
    )
}

class D { f "X" | "Y" }

test "string_literal_union_field_serializes_as_string" {
    // Primary ticket case: a `"X" | "Y"` field round-trips as the plain string.
    assert.equal(
        baml.json.stringify(baml.json.from(D { f: "X" })),
        "{\"f\":\"X\"}"
    )
}

class OptEnum { e E? }

test "optional_enum_field_serializes_via_typed_to_string" {
    // Regression: `E?` == `E | null` is a union, so the typed `to_string<T>`
    // walker rendered it via the untyped converter, which produced `null` for
    // the enum variant. It must be the variant name.
    assert.equal(baml.json.to_string(OptEnum { e: E.A }), "{\"e\":\"A\"}")
}

enum F { X  Y }

class EnumUnion { v E | F }

test "enum_union_field_serializes_via_typed_to_string" {
    // Regression: a field typed as a union of two enums (`E | F`) serialized
    // the held variant as `null` through the typed walker. It must be the name.
    assert.equal(baml.json.to_string(EnumUnion { v: F.Y }), "{\"v\":\"Y\"}")
}
