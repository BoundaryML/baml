// Tests for `reflect.Type.of<T>()` inside generic functions.
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
    reflect.Type.of<T>().to_string()
}

function describe_user_fn() -> string {
    describe<User>()
}

test "describe_user_returns_user" {
    assert.equal(describe_user_fn(), "reflect_type_of_generic.User")
}

function describe_int_fn() -> string {
    describe<int>()
}

test "describe_int_returns_int" {
    assert.equal(describe_int_fn(), "int")
}

function describe_string_fn() -> string {
    describe<string>()
}

test "describe_string_returns_string" {
    assert.equal(describe_string_fn(), "string")
}

// ─── Composite template tests ─────────────────────────────────────────────────

function array_of<T>() -> reflect.Type {
    reflect.Type.of<T[]>()
}

function array_of_user_to_string_fn() -> string {
    array_of<User>().to_string()
}

test "array_of_user_to_string" {
    assert.equal(array_of_user_to_string_fn(), "reflect_type_of_generic.User[]")
}

function array_of_int_fn() -> string {
    array_of<int>().to_string()
}

test "array_of_int_to_string" {
    assert.equal(array_of_int_fn(), "int[]")
}

// ─── Equality tests ───────────────────────────────────────────────────────────

function described_type<T>() -> reflect.Type {
    reflect.Type.of<T>()
}

function described_type_eq_concrete_fn() -> bool {
    described_type<User>() == reflect.Type.of<User>()
}

test "described_type_eq_concrete" {
    assert.is_true(described_type_eq_concrete_fn())
}

function described_type_neq_different_fn() -> bool {
    described_type<int>() == reflect.Type.of<User>()
}

test "described_type_neq_different" {
    assert.is_true(!described_type_neq_different_fn())
}

// ─── Closure captures type args ───────────────────────────────────────────────

function make_describer<T>() -> () -> string throws never {
    return () -> string { reflect.Type.of<T>().to_string() }
}

function closure_captures_type_arg_user_fn() -> string {
    let f = make_describer<User>();
    f()
}

test "closure_captures_type_arg_user" {
    assert.equal(closure_captures_type_arg_user_fn(), "reflect_type_of_generic.User")
}

function closure_captures_type_arg_int_fn() -> string {
    let f = make_describer<int>();
    f()
}

test "closure_captures_type_arg_int" {
    assert.equal(closure_captures_type_arg_int_fn(), "int")
}

// ─── Forwarding chain tests ───────────────────────────────────────────────────

function fwd<T>() -> reflect.Type {
    described_type<T>()
}

function fwd_user_eq_reflect_type_of_user_fn() -> bool {
    fwd<User>() == reflect.Type.of<User>()
}

test "fwd_user_eq_reflect_type_of_user" {
    assert.is_true(fwd_user_eq_reflect_type_of_user_fn())
}

function fwd_int_neq_user_fn() -> bool {
    fwd<int>() == reflect.Type.of<User>()
}

test "fwd_int_neq_user" {
    assert.is_true(!fwd_int_neq_user_fn())
}

function level1<T>() -> reflect.Type {
    reflect.Type.of<T>()
}

function level2<T>() -> reflect.Type {
    level1<T>()
}

function level3<T>() -> reflect.Type {
    level2<T>()
}

function three_level_forwarding_eq_concrete_fn() -> bool {
    level3<User>() == reflect.Type.of<User>()
}

test "three_level_forwarding_eq_concrete" {
    assert.is_true(three_level_forwarding_eq_concrete_fn())
}

function level1_str<T>() -> string {
    reflect.Type.of<T>().to_string()
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
    assert.equal(three_level_forwarding_to_string_fn(), "int")
}

// ─── Closure capturing composite type ─────────────────────────────────────────

function make_array_describer<T>() -> () -> string throws never {
    return () -> string { reflect.Type.of<T[]>().to_string() }
}

function closure_captures_type_arg_array_of_user_fn() -> string {
    let f = make_array_describer<User>();
    f()
}

test "closure_captures_type_arg_array_of_user" {
    assert.equal(closure_captures_type_arg_array_of_user_fn(), "reflect_type_of_generic.User[]")
}

function make_opt_describer<T>() -> () -> string throws never {
    return () -> string { reflect.Type.of<T?>().to_string() }
}

function closure_captures_type_arg_optional_of_int_fn() -> string {
    let f = make_opt_describer<int>();
    f()
}

test "closure_captures_type_arg_optional_of_int" {
    assert.equal(closure_captures_type_arg_optional_of_int_fn(), "int | null")
}

// ─── Depth-3 method-call paths ────────────────────────────────────────────────

class Box<T> {
    value T

    function describe(self) -> string {
        reflect.Type.of<T>().to_string()
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
    assert.equal(type_of_depth3_path_in_method_fn(), "int")
}

class Inner<U> {
    v U

    function name(self) -> string {
        reflect.Type.of<U>().to_string()
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
    assert.equal(type_of_depth3_picks_inner_type_param_fn(), "int")
}

class InnerShow<U> {
    v U

    function show(self) -> string {
        reflect.Type.of<U>().to_string()
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
    assert.equal(type_of_self_field_method_depth3_fn(), "string")
}

class Leaf<T> {
    v T

    function describe(self) -> string {
        reflect.Type.of<T>().to_string()
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
    assert.equal(type_of_depth4_path_picks_prefix_class_args_fn(), "int")
}
