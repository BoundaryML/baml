// Tests for `reflect.Type.of<T>()` with concrete type arguments.
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
    let s = reflect.Type.of<User>().to_string();
    assert.equal(s, "reflect_type_of.User")
}

test "type_of_int_to_string" {
    assert.equal(reflect.Type.of<int>().to_string(), "int")
}

test "type_of_string_to_string" {
    assert.equal(reflect.Type.of<string>().to_string(), "string")
}

test "type_of_bool_to_string" {
    assert.equal(reflect.Type.of<bool>().to_string(), "bool")
}

test "type_of_array_to_string" {
    let s = reflect.Type.of<User[]>().to_string();
    assert.equal(s, "reflect_type_of.User[]")
}

test "type_of_optional_to_string" {
    let s = reflect.Type.of<User?>().to_string();
    assert.equal(s, "reflect_type_of.User | null")
}

test "type_of_map_to_string" {
    let s = reflect.Type.of<map<string, User>>().to_string();
    assert.equal(s, "map<string, reflect_type_of.User>")
}

test "type_of_generic_class_runs_without_crash" {
    let s = reflect.Type.of<Container<User>>().to_string();
    assert.equal(s, "reflect_type_of.Container<reflect_type_of.User>")
}

test "type_of_generic_class_to_string_matches_source" {
    let matches = reflect.Type.of<Container<User>>().to_string() == "reflect_type_of.Container<reflect_type_of.User>";
    assert.is_true(matches)
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_map_eq" {
    assert.is_true(reflect.Type.of<map<string, User>>() == reflect.Type.of<map<string, User>>())
}

test "type_of_generic_class_eq" {
    assert.is_true(reflect.Type.of<Container<User>>() == reflect.Type.of<Container<User>>())
}

test "type_of_generic_class_neq_different_arg" {
    assert.is_true(reflect.Type.of<Container<User>>() != reflect.Type.of<Container<int>>())
}

// ─── Phase 7: parametric class forwarding via generic bodies ─────────────────

// Kept as a top-level generic function: forwarding a type parameter through a
// generic body into `reflect.Type.of` is itself the feature under test here.
function wrap_in_container<T>() -> reflect.Type {
    reflect.Type.of<Container<T>>()
}

test "wrap_in_container_to_string" {
    let s = wrap_in_container<int>().to_string();
    assert.equal(s, "reflect_type_of.Container<int>")
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_same_class_eq" {
    assert.is_true(reflect.Type.of<User>() == reflect.Type.of<User>())
}

test "type_of_different_class_neq" {
    assert.is_true(reflect.Type.of<User>() != reflect.Type.of<int>())
}

test "type_of_assign_and_compare" {
    // Locals holding `type` values, compared inside a test block: the exact
    // shape the pre-#3782 lowering bug broke (locals lowered to `const null`,
    // making this compare `null == null`).
    let a: reflect.Type = reflect.Type.of<User>();
    let b: reflect.Type = reflect.Type.of<User>();
    assert.is_true(a == b)
}

// ─── Enum and union coverage ─────────────────────────────────────────────────

test "type_of_enum_to_string" {
    let s = reflect.Type.of<RgbColor>().to_string();
    assert.equal(s, "reflect_type_of.RgbColor")
}

test "type_of_enum_eq" {
    assert.is_true(reflect.Type.of<RgbColor>() == reflect.Type.of<RgbColor>())
}

test "type_of_enum_neq_class" {
    assert.is_true(reflect.Type.of<RgbColor>() != reflect.Type.of<User>())
}

test "type_of_union_eq" {
    assert.is_true(reflect.Type.of<int | string>() == reflect.Type.of<int | string>())
}

test "type_of_union_neq_different_member" {
    assert.is_true(reflect.Type.of<int | string>() != reflect.Type.of<int | bool>())
}

// ─── Stable identity via .to_string() ───────────────────────────────────────

test "type_of_to_string_works_as_map_key" {
    let m: map<string, int> = {};
    m[reflect.Type.of<User>().to_string()] = 1;
    m[reflect.Type.of<int>().to_string()] = 2;
    assert.equal(m[reflect.Type.of<User>().to_string()], 1)
}

// ─── reflect.Type.of_value (BEP-066 K-13) ────────────────────────────────────────────

test "of_value_int" {
    assert.is_true(reflect.Type.of_value(5) == reflect.Type.of<int>())
}

test "of_value_never_yields_a_literal_type" {
    // K-12: the reconstructed type is the value type, never the literal.
    assert.equal(reflect.Type.of_value(5).to_string(), "int");
    assert.equal(reflect.Type.of_value("hi").to_string(), "string");
    assert.equal(reflect.Type.of_value(true).to_string(), "bool")
}

test "of_value_string_and_bool" {
    assert.is_true(reflect.Type.of_value("hi") == reflect.Type.of<string>());
    assert.is_true(reflect.Type.of_value(true) == reflect.Type.of<bool>());
    assert.is_true(reflect.Type.of_value(null) == reflect.Type.of<null>())
}

test "of_value_class_instance" {
    let u = User { name: "a" };
    assert.is_true(reflect.Type.of_value(u) == reflect.Type.of<User>());
    assert.equal(reflect.Type.of_value(u).to_string(), "reflect_type_of.User")
}

test "of_value_array_reports_element_type" {
    let xs: int[] = [1, 2];
    assert.is_true(reflect.Type.of_value(xs) == reflect.Type.of<int[]>())
}

test "of_value_of_a_type_value_is_precise_kind" {
    // A reflected class type value has the sealed class-kind view as its concrete type.
    assert.is_true(reflect.Type.of_value(reflect.Type.of<User>()) == reflect.Type.of<reflect.class.Type>())
}

test "of_value_widened_unknown_slot" {
    // Through an `unknown`-typed slot the runtime tag still drives the answer.
    let v: unknown = 42;
    assert.is_true(reflect.Type.of_value(v) == reflect.Type.of<int>())
}

function of_value_helper(q: string) -> string throws never {
    return q
}

test "of_value_function_value_reports_function_type" {
    let f: baml.AnyFunction = of_value_helper;
    let t = reflect.Type.of_value(f);
    // The reconstructed signature type mentions the arrow, not `unknown`.
    assert.is_true(t != reflect.Type.of<unknown>())
}

// ─── `reflect.signature` reads a function value's own signature ─────────────
//
// `reflect` is an ordinary root package now (#4543), so this is ordinary
// package routing — there is no second, qualified spelling for it to agree
// with. What is worth pinning is the content: the reconstructed signature
// reports the declared name, return type, and error type.

test "reflect_signature_reports_the_declared_types" {
    let f: baml.AnyFunction = of_value_helper;
    let sig = reflect.signature(f);
    // `name` is null for host and compiler-synthesized callables; a declared
    // function reports the name it was written with.
    assert.equal(sig.name ?? "<unnamed>", "of_value_helper");
    assert.equal(sig.returns.to_string(), "string");
    assert.is_true(sig.returns == reflect.Type.of<string>());
    // `of_value_helper` declares `throws never`.
    assert.is_true(sig.errors == reflect.Type.of<never>())
}
