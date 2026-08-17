// Tests for `type.of<T>()` with concrete type arguments.
//
// Converted from crates/baml_tests/tests/reflect_type_of.rs.
//
// The Rust tests assert plain names like "User" because the test harness
// uses a flat (no-subdirectory) source layout with package name "user",
// giving an unqualified class name. In a BAML project file under
// ns_reflect_type_of/, the namespace segment is prepended: class `User`
// becomes "reflect_type_of.User", enum `RgbColor` becomes
// "reflect_type_of.RgbColor", etc.  Primitive types (int, string, bool) and
// map/array/optional wrappers around them remain unqualified.
//
// Test bodies live directly in the `test` blocks. They were originally
// lifted into top-level helper functions to dodge the "test-block
// local-boxing VM bug" (test-body `let` locals never resolved and lowered
// to `const null` — fixed by #3782, see
// thoughts/antonio/s1-vm-bug-diagnosis.md). Inlined bodies now double as
// the regression net for that fix.

class User { name string }
class Container<T> { value T }
// Renamed from Color to RgbColor to avoid collision with ns_json_to_from_string.Color,
// which causes baml.json.from_string<Color> to become ambiguous across namespaces.
enum RgbColor { Red, Green, Blue }

// ─── Basic to_string tests ───────────────────────────────────────────────────

test "type_of_class_to_string" {
    let s = type.of<User>().to_string();
    assert.equal(s, "reflect_type_of.User")
}

test "type_of_int_to_string" {
    assert.equal(type.of<int>().to_string(), "int")
}

test "type_of_string_to_string" {
    assert.equal(type.of<string>().to_string(), "string")
}

test "type_of_bool_to_string" {
    assert.equal(type.of<bool>().to_string(), "bool")
}

test "type_of_array_to_string" {
    let s = type.of<User[]>().to_string();
    assert.equal(s, "reflect_type_of.User[]")
}

test "type_of_optional_to_string" {
    let s = type.of<User?>().to_string();
    assert.equal(s, "reflect_type_of.User | null")
}

test "type_of_map_to_string" {
    let s = type.of<map<string, User>>().to_string();
    assert.equal(s, "map<string, reflect_type_of.User>")
}

test "type_of_generic_class_runs_without_crash" {
    let s = type.of<Container<User>>().to_string();
    assert.equal(s, "reflect_type_of.Container<reflect_type_of.User>")
}

test "type_of_generic_class_to_string_matches_source" {
    let matches = type.of<Container<User>>().to_string() == "reflect_type_of.Container<reflect_type_of.User>";
    assert.is_true(matches)
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_map_eq" {
    assert.is_true(type.of<map<string, User>>() == type.of<map<string, User>>())
}

test "type_of_generic_class_eq" {
    assert.is_true(type.of<Container<User>>() == type.of<Container<User>>())
}

test "type_of_generic_class_neq_different_arg" {
    assert.is_true(type.of<Container<User>>() != type.of<Container<int>>())
}

// ─── Phase 7: parametric class forwarding via generic bodies ─────────────────

// Kept as a top-level generic function: forwarding a type parameter through a
// generic body into `type.of` is itself the feature under test here.
function wrap_in_container<T>() -> type {
    type.of<Container<T>>()
}

test "wrap_in_container_to_string" {
    let s = wrap_in_container<int>().to_string();
    assert.equal(s, "reflect_type_of.Container<int>")
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_same_class_eq" {
    assert.is_true(type.of<User>() == type.of<User>())
}

test "type_of_different_class_neq" {
    assert.is_true(type.of<User>() != type.of<int>())
}

test "type_of_assign_and_compare" {
    // Locals holding `type` values, compared inside a test block: the exact
    // shape the pre-#3782 lowering bug broke (locals lowered to `const null`,
    // making this compare `null == null`).
    let a: type = type.of<User>();
    let b: type = type.of<User>();
    assert.is_true(a == b)
}

// ─── Enum and union coverage ─────────────────────────────────────────────────

test "type_of_enum_to_string" {
    let s = type.of<RgbColor>().to_string();
    assert.equal(s, "reflect_type_of.RgbColor")
}

test "type_of_enum_eq" {
    assert.is_true(type.of<RgbColor>() == type.of<RgbColor>())
}

test "type_of_enum_neq_class" {
    assert.is_true(type.of<RgbColor>() != type.of<User>())
}

test "type_of_union_eq" {
    assert.is_true(type.of<int | string>() == type.of<int | string>())
}

test "type_of_union_neq_different_member" {
    assert.is_true(type.of<int | string>() != type.of<int | bool>())
}

// ─── Stable identity via .to_string() ───────────────────────────────────────

test "type_of_to_string_works_as_map_key" {
    let m: map<string, int> = {};
    m[type.of<User>().to_string()] = 1;
    m[type.of<int>().to_string()] = 2;
    assert.equal(m[type.of<User>().to_string()], 1)
}

// ─── type.of_value (BEP-066 K-13) ────────────────────────────────────────────

test "of_value_int" {
    assert.is_true(type.of_value(5) == type.of<int>())
}

test "of_value_never_yields_a_literal_type" {
    // K-12: the reconstructed type is the value type, never the literal.
    assert.equal(type.of_value(5).to_string(), "int");
    assert.equal(type.of_value("hi").to_string(), "string");
    assert.equal(type.of_value(true).to_string(), "bool")
}

test "of_value_string_and_bool" {
    assert.is_true(type.of_value("hi") == type.of<string>());
    assert.is_true(type.of_value(true) == type.of<bool>());
    assert.is_true(type.of_value(null) == type.of<null>())
}

test "of_value_class_instance" {
    let u = User { name: "a" };
    assert.is_true(type.of_value(u) == type.of<User>());
    assert.equal(type.of_value(u).to_string(), "reflect_type_of.User")
}

test "of_value_array_reports_element_type" {
    let xs: int[] = [1, 2];
    assert.is_true(type.of_value(xs) == type.of<int[]>())
}

test "of_value_of_a_type_value_is_precise_kind" {
    // A reflected class type value has the sealed class-kind view as its concrete type.
    assert.is_true(type.of_value(type.of<User>()) == type.of<baml.reflect.class.Type>())
}

test "of_value_widened_unknown_slot" {
    // Through an `unknown`-typed slot the runtime tag still drives the answer.
    let v: unknown = 42;
    assert.is_true(type.of_value(v) == type.of<int>())
}

function of_value_helper(q: string) -> string throws never {
    return q
}

test "of_value_function_value_reports_function_type" {
    let f: baml.AnyFunction = of_value_helper;
    let t = type.of_value(f);
    // The reconstructed signature type mentions the arrow, not `unknown`.
    assert.is_true(t != type.of<unknown>())
}

// ─── `reflect` shorthand and its qualified form (BEP-066 I-9) ────────────────

test "reflect_signature_bare_and_qualified_agree" {
    let f: baml.AnyFunction = of_value_helper;
    let bare = reflect.signature(f);
    let qualified = baml.reflect.signature(f);
    assert.equal(bare.returns.to_string(), "string");
    assert.is_true(bare.returns == qualified.returns);
    assert.is_true(bare.errors == qualified.errors)
}
