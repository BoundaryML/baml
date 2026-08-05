// Tests for `reflect.type_of<T>()` with concrete type arguments.
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
    let s = reflect.type_of<User>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.User"))
}

test "type_of_int_to_string" {
    assert.is_true(baml.deep_equals(reflect.type_of<int>().to_string(), "int"))
}

test "type_of_string_to_string" {
    assert.is_true(baml.deep_equals(reflect.type_of<string>().to_string(), "string"))
}

test "type_of_bool_to_string" {
    assert.is_true(baml.deep_equals(reflect.type_of<bool>().to_string(), "bool"))
}

test "type_of_array_to_string" {
    let s = reflect.type_of<User[]>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.User[]"))
}

test "type_of_optional_to_string" {
    let s = reflect.type_of<User?>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.User | null"))
}

test "type_of_map_to_string" {
    let s = reflect.type_of<map<string, User>>().to_string();
    assert.is_true(baml.deep_equals(s, "map<string, reflect_type_of.User>"))
}

test "type_of_generic_class_runs_without_crash" {
    let s = reflect.type_of<Container<User>>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.Container<reflect_type_of.User>"))
}

test "type_of_generic_class_to_string_matches_source" {
    let matches = reflect.type_of<Container<User>>().to_string() == "reflect_type_of.Container<reflect_type_of.User>";
    assert.is_true(baml.deep_equals(matches, true))
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_map_eq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<map<string, User>>() == reflect.type_of<map<string, User>>(),
        true
    ))
}

test "type_of_generic_class_eq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<Container<User>>() == reflect.type_of<Container<User>>(),
        true
    ))
}

test "type_of_generic_class_neq_different_arg" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<Container<User>>() == reflect.type_of<Container<int>>(),
        false
    ))
}

// ─── Phase 7: parametric class forwarding via generic bodies ─────────────────

// Kept as a top-level generic function: forwarding a type parameter through a
// generic body into `reflect.type_of` is itself the feature under test here.
function wrap_in_container<T>() -> type {
    reflect.type_of<Container<T>>()
}

test "wrap_in_container_to_string" {
    let s = wrap_in_container<int>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.Container<int>"))
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_same_class_eq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<User>() == reflect.type_of<User>(),
        true
    ))
}

test "type_of_different_class_neq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<User>() == reflect.type_of<int>(),
        false
    ))
}

test "type_of_assign_and_compare" {
    // Locals holding `type` values, compared inside a test block: the exact
    // shape the pre-#3782 lowering bug broke (locals lowered to `const null`,
    // making this compare `null == null`).
    let a: type = reflect.type_of<User>();
    let b: type = reflect.type_of<User>();
    assert.is_true(baml.deep_equals(a == b, true))
}

// ─── Enum and union coverage ─────────────────────────────────────────────────

test "type_of_enum_to_string" {
    let s = reflect.type_of<RgbColor>().to_string();
    assert.is_true(baml.deep_equals(s, "reflect_type_of.RgbColor"))
}

test "type_of_enum_eq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<RgbColor>() == reflect.type_of<RgbColor>(),
        true
    ))
}

test "type_of_enum_neq_class" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<RgbColor>() == reflect.type_of<User>(),
        false
    ))
}

test "type_of_union_eq" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<int | string>() == reflect.type_of<int | string>(),
        true
    ))
}

test "type_of_union_neq_different_member" {
    assert.is_true(baml.deep_equals(
        reflect.type_of<int | string>() == reflect.type_of<int | bool>(),
        false
    ))
}

// ─── Stable identity via .to_string() ───────────────────────────────────────

test "type_of_to_string_works_as_map_key" {
    let m: map<string, int> = {};
    m[reflect.type_of<User>().to_string()] = 1;
    m[reflect.type_of<int>().to_string()] = 2;
    assert.is_true(baml.deep_equals(m[reflect.type_of<User>().to_string()], 1))
}
