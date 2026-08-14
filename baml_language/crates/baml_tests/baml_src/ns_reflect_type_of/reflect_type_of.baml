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
// Each test body goes into a top-level helper function so that locals are
// not subject to the test-block boxing bug.

class User { name string }
class Container<T> { value T }
// Renamed from Color to RgbColor to avoid collision with ns_json_to_from_string.Color,
// which causes baml.json.from_string<Color> to become ambiguous across namespaces.
enum RgbColor { Red, Green, Blue }

// ─── Basic to_string tests ───────────────────────────────────────────────────

function type_of_class_to_string_fn() -> string {
    reflect.type_of<User>().to_string()
}

test "type_of_class_to_string" {
    assert.equal(type_of_class_to_string_fn(), "reflect_type_of.User")
}

test "type_of_int_to_string" {
    assert.equal(reflect.type_of<int>().to_string(), "int")
}

test "type_of_string_to_string" {
    assert.equal(reflect.type_of<string>().to_string(), "string")
}

test "type_of_bool_to_string" {
    assert.equal(reflect.type_of<bool>().to_string(), "bool")
}

function type_of_array_to_string_fn() -> string {
    reflect.type_of<User[]>().to_string()
}

test "type_of_array_to_string" {
    assert.equal(type_of_array_to_string_fn(), "reflect_type_of.User[]")
}

function type_of_optional_to_string_fn() -> string {
    reflect.type_of<User?>().to_string()
}

test "type_of_optional_to_string" {
    assert.equal(type_of_optional_to_string_fn(), "reflect_type_of.User | null")
}

function type_of_map_to_string_fn() -> string {
    reflect.type_of<map<string, User>>().to_string()
}

test "type_of_map_to_string" {
    assert.equal(type_of_map_to_string_fn(), "map<string, reflect_type_of.User>")
}

function type_of_generic_class_runs_without_crash_fn() -> string {
    reflect.type_of<Container<User>>().to_string()
}

test "type_of_generic_class_runs_without_crash" {
    assert.equal(
        type_of_generic_class_runs_without_crash_fn(),
        "reflect_type_of.Container<reflect_type_of.User>",
    )
}

function type_of_generic_class_to_string_matches_source_fn() -> bool {
    reflect.type_of<Container<User>>().to_string() == "reflect_type_of.Container<reflect_type_of.User>"
}

test "type_of_generic_class_to_string_matches_source" {
    assert.is_true(type_of_generic_class_to_string_matches_source_fn())
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_map_eq" {
    assert.is_true(
        reflect.type_of<map<string, User>>() == reflect.type_of<map<string, User>>(),
    )
}

test "type_of_generic_class_eq" {
    assert.is_true(reflect.type_of<Container<User>>() == reflect.type_of<Container<User>>())
}

test "type_of_generic_class_neq_different_arg" {
    assert.is_true(reflect.type_of<Container<User>>() != reflect.type_of<Container<int>>())
}

// ─── Phase 7: parametric class forwarding via generic bodies ─────────────────

function wrap_in_container<T>() -> type {
    reflect.type_of<Container<T>>()
}

function wrap_in_container_to_string_fn() -> string {
    wrap_in_container<int>().to_string()
}

test "wrap_in_container_to_string" {
    assert.equal(wrap_in_container_to_string_fn(), "reflect_type_of.Container<int>")
}

// ─── Structural equality ─────────────────────────────────────────────────────

test "type_of_same_class_eq" {
    assert.is_true(reflect.type_of<User>() == reflect.type_of<User>())
}

test "type_of_different_class_neq" {
    assert.is_true(reflect.type_of<User>() != reflect.type_of<int>())
}

function type_of_assign_and_compare_fn() -> bool {
    let a: type = reflect.type_of<User>();
    let b: type = reflect.type_of<User>();
    a == b
}

test "type_of_assign_and_compare" {
    assert.is_true(type_of_assign_and_compare_fn())
}

// ─── Enum and union coverage ─────────────────────────────────────────────────

function type_of_enum_to_string_fn() -> string {
    reflect.type_of<RgbColor>().to_string()
}

test "type_of_enum_to_string" {
    assert.equal(type_of_enum_to_string_fn(), "reflect_type_of.RgbColor")
}

test "type_of_enum_eq" {
    assert.is_true(reflect.type_of<RgbColor>() == reflect.type_of<RgbColor>())
}

test "type_of_enum_neq_class" {
    assert.is_true(reflect.type_of<RgbColor>() != reflect.type_of<User>())
}

test "type_of_union_eq" {
    assert.is_true(reflect.type_of<int | string>() == reflect.type_of<int | string>())
}

test "type_of_union_neq_different_member" {
    assert.is_true(reflect.type_of<int | string>() != reflect.type_of<int | bool>())
}

// ─── Stable identity via .to_string() ───────────────────────────────────────

function type_of_to_string_works_as_map_key_fn() -> int {
    let m: map<string, int> = {};
    m[reflect.type_of<User>().to_string()] = 1;
    m[reflect.type_of<int>().to_string()] = 2;
    m[reflect.type_of<User>().to_string()]
}

test "type_of_to_string_works_as_map_key" {
    assert.equal(type_of_to_string_works_as_map_key_fn(), 1)
}
