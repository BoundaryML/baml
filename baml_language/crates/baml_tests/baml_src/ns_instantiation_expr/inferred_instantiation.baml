// A generic function referenced where the expected function type pins its
// type arguments (`let c: (int) -> int = foo`) is the same realized value as
// the written `foo<int>`: it carries its type arguments, states its
// specialized type to `is`, compares equal to the written spelling, and seeds
// the frame of a body that reifies them. The host-boundary half (calling the
// value through a handle, passing it back) is in
// `bex_engine/tests/implicit_function_values.rs`.

function inferred_id<T>(x: T) -> T { x }

function inferred_reflect<T>(x: T) -> string { reflect.Type.of<T>().to_string() }

function inferred_empty<T>() -> T[] { [] }

function inferred_apply<T>(x: T, f: (T) -> T) -> T { f(x) }

function get_inferred_id() -> (int) -> int throws never { inferred_id }

function get_inferred_reflect() -> (int) -> string throws never { inferred_reflect }

function get_inferred_empty() -> () -> string[] throws never { inferred_empty }

function apply_int(f: (int) -> int throws never) -> int { f(4) }

test "an_inferred_function_value_runs" {
    assert.equal(get_inferred_id()(3), 3)
}

test "an_inferred_function_value_has_its_specialized_type" {
    assert.is_true(get_inferred_id() is (int) -> int throws never)
}

test "an_inferred_function_value_is_not_a_member_of_another_instantiation" {
    assert.is_true(!(get_inferred_id() is (string) -> string throws never))
}

test "an_inferred_function_value_equals_the_written_instantiation" {
    assert.is_true(get_inferred_id() == inferred_id<int>)
}

test "an_inferred_function_value_seeds_a_reified_type_argument" {
    assert.equal(get_inferred_reflect()(1), "int")
}

// `[]` at `T[]` realizes `T` from the frame; an unseeded frame is a VM error.
test "an_inferred_function_value_seeds_a_type_argument_only_its_return_names" {
    assert.equal(get_inferred_empty()().length(), 0)
}

test "an_annotated_let_infers_the_instantiation" {
    let c: (int) -> int throws never = inferred_id;
    assert.is_true(c is (int) -> int throws never)
}

test "an_argument_position_infers_the_instantiation" {
    assert.equal(apply_int(inferred_id), 4)
}

// The synthetic effect parameter of a callback has no runtime slot, so the
// inferred value is the very value the written spelling makes.
test "an_inferred_effect_polymorphic_value_equals_the_written_instantiation" {
    let f: (int, (int) -> int throws never) -> int throws never = inferred_apply;
    assert.is_true(f == inferred_apply<int>)
}

// Inside a generic function the instantiation names the enclosing `T`, so the
// value is built at run time from the current frame.
function inferred_forward_reflect<T>(x: T) -> string {
    let f: (T) -> string throws never = inferred_reflect;
    f(x)
}

test "an_inferred_instantiation_over_an_enclosing_type_parameter_resolves_it" {
    assert.equal(inferred_forward_reflect<string>("a"), "string")
}

class InferredHolder {
    function pick<U>(self, u: U) -> U { u }
    function make<U>(u: U) -> U { u }
    function reflect_own<U>(self, u: U) -> string { reflect.Type.of<U>().to_string() }
}

class InferredBox<T> {
    v T
    function wrap(x: T) -> InferredBox<T> { InferredBox { v: x } }
}

test "an_inferred_static_method_value_has_its_specialized_type" {
    let f: (int) -> int throws never = InferredHolder.make;
    assert.is_true(f is (int) -> int throws never)
}

test "an_inferred_unbound_method_value_has_its_specialized_type" {
    let f: (InferredHolder, int) -> int throws never = InferredHolder.pick;
    assert.is_true(f is (InferredHolder, int) -> int throws never)
}

test "an_inferred_bound_method_value_has_its_specialized_type" {
    let h = InferredHolder {};
    let f: (int) -> int throws never = h.pick;
    assert.is_true(f is (int) -> int throws never)
}

test "an_inferred_bound_method_value_equals_the_written_instantiation" {
    let h = InferredHolder {};
    let f: (int) -> int throws never = h.pick;
    assert.is_true(f == h.pick<int>)
}

test "an_inferred_bound_method_value_seeds_its_own_type_argument" {
    let h = InferredHolder {};
    let f: (bool) -> string throws never = h.reflect_own;
    assert.equal(f(true), "bool")
}

// A generic class's static method has no receiver to supply the class frame,
// so the value carries it.
test "an_inferred_generic_class_static_value_carries_the_class_frame" {
    let f: (int) -> InferredBox<int> throws never = InferredBox.wrap;
    assert.is_true(f is (int) -> InferredBox<int> throws never);
    assert.equal(f(5).v, 5)
}

interface InferredPicker {
    function choose<U>(self, u: U) -> U throws never;
}

class InferredChooser {}

implements InferredPicker for InferredChooser {
    function choose<U>(self, u: U) -> U throws never { u }
}

function inferred_choose_through<T extends InferredPicker>(t: T) -> (int) -> int throws never {
    t.choose
}

test "an_inferred_interface_method_value_has_its_specialized_type" {
    let c = InferredChooser {};
    let f: (int) -> int throws never = c.choose;
    assert.is_true(f is (int) -> int throws never);
    assert.is_true(inferred_choose_through(c) is (int) -> int throws never)
}
