// Tests for `reflect.type_of<T>()` inside generic functions.
//
// Converted from crates/baml_tests/tests/reflect_type_of_generic.rs.
//
// In this namespace (ns_reflect_type_of_generic), class User becomes
// "reflect_type_of_generic.User". Primitive types (int, string) remain
// unqualified. The Rust tests assert bare names like "User" because they
// run in a flat source layout with package "user" and no namespace.
//
// Each body is lifted into a top-level helper function to avoid the
// test-block local-boxing VM bug (especially for closures and instances).

class User { name string }

// ─── Bare-T tests ─────────────────────────────────────────────────────────────

function describe<T>() -> string {
    reflect.type_of<T>().to_string()
}

function describe_user_fn() -> string {
    describe<User>()
}

test "describe_user_returns_user" {
    assert.is_true(baml.deep_equals(describe_user_fn(), "reflect_type_of_generic.User"))
}

function describe_int_fn() -> string {
    describe<int>()
}

test "describe_int_returns_int" {
    assert.is_true(baml.deep_equals(describe_int_fn(), "int"))
}

function describe_string_fn() -> string {
    describe<string>()
}

test "describe_string_returns_string" {
    assert.is_true(baml.deep_equals(describe_string_fn(), "string"))
}

// ─── Composite template tests ─────────────────────────────────────────────────

function array_of<T>() -> type {
    reflect.type_of<T[]>()
}

function array_of_user_to_string_fn() -> string {
    array_of<User>().to_string()
}

test "array_of_user_to_string" {
    assert.is_true(baml.deep_equals(array_of_user_to_string_fn(), "reflect_type_of_generic.User[]"))
}

function array_of_int_fn() -> string {
    array_of<int>().to_string()
}

test "array_of_int_to_string" {
    assert.is_true(baml.deep_equals(array_of_int_fn(), "int[]"))
}

// ─── Equality tests ───────────────────────────────────────────────────────────

function described_type<T>() -> type {
    reflect.type_of<T>()
}

function described_type_eq_concrete_fn() -> bool {
    described_type<User>() == reflect.type_of<User>()
}

test "described_type_eq_concrete" {
    assert.is_true(baml.deep_equals(described_type_eq_concrete_fn(), true))
}

function described_type_neq_different_fn() -> bool {
    described_type<int>() == reflect.type_of<User>()
}

test "described_type_neq_different" {
    assert.is_true(baml.deep_equals(described_type_neq_different_fn(), false))
}

// ─── Closure captures type args ───────────────────────────────────────────────

function make_describer<T>() -> () -> string {
    return () -> string { reflect.type_of<T>().to_string() }
}

function closure_captures_type_arg_user_fn() -> string {
    let f = make_describer<User>();
    f()
}

test "closure_captures_type_arg_user" {
    assert.is_true(baml.deep_equals(closure_captures_type_arg_user_fn(), "reflect_type_of_generic.User"))
}

function closure_captures_type_arg_int_fn() -> string {
    let f = make_describer<int>();
    f()
}

test "closure_captures_type_arg_int" {
    assert.is_true(baml.deep_equals(closure_captures_type_arg_int_fn(), "int"))
}

// ─── Forwarding chain tests ───────────────────────────────────────────────────

function fwd<T>() -> type {
    described_type<T>()
}

function fwd_user_eq_reflect_type_of_user_fn() -> bool {
    fwd<User>() == reflect.type_of<User>()
}

test "fwd_user_eq_reflect_type_of_user" {
    assert.is_true(baml.deep_equals(fwd_user_eq_reflect_type_of_user_fn(), true))
}

function fwd_int_neq_user_fn() -> bool {
    fwd<int>() == reflect.type_of<User>()
}

test "fwd_int_neq_user" {
    assert.is_true(baml.deep_equals(fwd_int_neq_user_fn(), false))
}

function level1<T>() -> type {
    reflect.type_of<T>()
}

function level2<T>() -> type {
    level1<T>()
}

function level3<T>() -> type {
    level2<T>()
}

function three_level_forwarding_eq_concrete_fn() -> bool {
    level3<User>() == reflect.type_of<User>()
}

test "three_level_forwarding_eq_concrete" {
    assert.is_true(baml.deep_equals(three_level_forwarding_eq_concrete_fn(), true))
}

function level1_str<T>() -> string {
    reflect.type_of<T>().to_string()
}

function level2_str<T>() -> string {
    level1_str<T>()
}

function level3_str<T>() -> string {
    level2_str<T>()
}

function three_level_forwarding_to_string_fn() -> string {
    level3_str<int>()
}

test "three_level_forwarding_to_string" {
    assert.is_true(baml.deep_equals(three_level_forwarding_to_string_fn(), "int"))
}

// ─── Closure capturing composite type ─────────────────────────────────────────

function make_array_describer<T>() -> () -> string {
    return () -> string { reflect.type_of<T[]>().to_string() }
}

function closure_captures_type_arg_array_of_user_fn() -> string {
    let f = make_array_describer<User>();
    f()
}

test "closure_captures_type_arg_array_of_user" {
    assert.is_true(baml.deep_equals(closure_captures_type_arg_array_of_user_fn(), "reflect_type_of_generic.User[]"))
}

function make_opt_describer<T>() -> () -> string {
    return () -> string { reflect.type_of<T?>().to_string() }
}

function closure_captures_type_arg_optional_of_int_fn() -> string {
    let f = make_opt_describer<int>();
    f()
}

test "closure_captures_type_arg_optional_of_int" {
    assert.is_true(baml.deep_equals(closure_captures_type_arg_optional_of_int_fn(), "int?"))
}

// ─── Depth-3 method-call paths ────────────────────────────────────────────────

class Box<T> {
    value T

    function describe(self) -> string {
        reflect.type_of<T>().to_string()
    }
}

class Holder {
    box Box<int>
}

function type_of_depth3_path_in_method_fn() -> string {
    let h: Holder = Holder { box: Box<int> { value: 42 } };
    h.box.describe()
}

test "type_of_depth3_path_in_method" {
    assert.is_true(baml.deep_equals(type_of_depth3_path_in_method_fn(), "int"))
}

class Inner<U> {
    v U

    function name(self) -> string {
        reflect.type_of<U>().to_string()
    }
}

class Outer<T> {
    inner Inner<T>

    function outer_describe(self) -> string {
        self.inner.name()
    }
}

function type_of_depth3_picks_inner_type_param_fn() -> string {
    let o: Outer<int> = Outer<int> { inner: Inner<int> { v: 7 } };
    o.outer_describe()
}

test "type_of_depth3_picks_inner_type_param" {
    assert.is_true(baml.deep_equals(type_of_depth3_picks_inner_type_param_fn(), "int"))
}

class InnerShow<U> {
    v U

    function show(self) -> string {
        reflect.type_of<U>().to_string()
    }
}

class HolderStr<T> {
    inner InnerShow<T>

    function delegate(self) -> string {
        self.inner.show()
    }
}

function type_of_self_field_method_depth3_fn() -> string {
    let h: HolderStr<string> = HolderStr<string> { inner: InnerShow<string> { v: "x" } };
    h.delegate()
}

test "type_of_self_field_method_depth3" {
    assert.is_true(baml.deep_equals(type_of_self_field_method_depth3_fn(), "string"))
}

class Leaf<T> {
    v T

    function describe(self) -> string {
        reflect.type_of<T>().to_string()
    }
}

class Mid {
    leaf Leaf<int>
}

class Root {
    mid Mid
}

function type_of_depth4_path_picks_prefix_class_args_fn() -> string {
    let r: Root = Root { mid: Mid { leaf: Leaf<int> { v: 1 } } };
    r.mid.leaf.describe()
}

test "type_of_depth4_path_picks_prefix_class_args" {
    assert.is_true(baml.deep_equals(type_of_depth4_path_picks_prefix_class_args_fn(), "int"))
}


function foo<T1, T2>(a: T1, b: T2) -> {
    bar(b, a)
}

function boo<S1, S2>(c: S1, d: S2) -> {
    reflect.type_of<S1>().to_string() + reflect.type_of<S2>().to_string()
}