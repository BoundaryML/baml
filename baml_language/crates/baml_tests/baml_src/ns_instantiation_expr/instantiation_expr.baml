// Runtime tests for generic instantiation expressions: `let f = foo<int>`.
//
// `foo<int>` (concrete type args) is a pooled, interned value: identical
// instantiations share one object (pointer-stable identity), and calling the
// value seeds frame.type_args so type-reifying bodies (reflect.type_of<T>,
// json natives) resolve T at runtime — i.e. it behaves exactly like the
// equivalent direct call `foo<int>(...)`.

function identity<T>(x: T) -> T { x }

// The value runs end-to-end: calling it invokes the underlying function.
function identity_int_value_fn() -> int {
    let f = identity<int>;
    f(5)
}

test "identity_int_value_runs" {
    assert.is_true(baml.deep_equals(identity_int_value_fn(), 5))
}

// Pointer-stable identity: two evaluations of `identity<int>` are interned to
// the SAME pooled object, and a different specialization is a different object.
function identity_same_specialization() -> bool {
    let a = identity<int>;
    let b = identity<int>;
    baml.deep_equals(a, b)
}

test "same_specialization_is_identical" {
    assert.is_true(identity_same_specialization())
}

// A deep-copied generic-function value compares equal to the original: the
// pooled/interned case short-circuits on pointer identity, and `deep_equals`
// also compares non-pooled copies structurally (function + type args).
function identity_deep_copy_equal() -> bool {
    let f = identity<int>;
    baml.deep_equals(f, baml.deep_copy(f))
}

test "deep_copy_compares_equal" {
    assert.is_true(identity_deep_copy_equal())
}

// Multiple bound type args: `pair<int, string>` specializes both params and
// runs end-to-end, returning the second (string) argument.
function pair<A, B>(a: A, b: B) -> B { b }

function pair_value_fn() -> string {
    let f = pair<int, string>;
    f(1, "hello")
}

test "pair_two_type_args_value_runs" {
    assert.is_true(baml.deep_equals(pair_value_fn(), "hello"))
}

class User { name string }

function describe<T>() -> string {
    reflect.type_of<T>().to_string()
}

// reflect.type_of<T> through an UNCALLED value now resolves T correctly (the
// value carries its type args and seeds frame.type_args when called).
function describe_user_value_fn() -> string {
    let f = describe<User>;
    f()
}

test "reflect_type_of_through_value" {
    assert.is_true(baml.deep_equals(describe_user_value_fn(), "instantiation_expr.User"))
}

// Type-reifying native through a value: `baml.json.from_string<User>` decodes
// using T=User seeded from the value (previously a runtime crash under erasure).
function from_string_value_fn() -> User {
    let f = baml.json.from_string<User>;
    f("{\"name\":\"Ada\"}")
}

test "json_native_through_value" {
    assert.is_true(baml.deep_equals(from_string_value_fn(), User { name: "Ada" }))
}

// Param-dependent instantiation (`identity<T>` where T is the enclosing
// function's type param) is built at runtime (MakeGenericFunction resolves the
// type arg against the current frame), so it runs AND reflects on T correctly.
function forward<T>(x: T) -> T {
    let f = identity<T>;
    f(x)
}

test "param_dependent_instantiation_runs" {
    assert.is_true(baml.deep_equals(forward<int>(7), 7))
}

// Param-dependent value whose body REFLECTS on T: the runtime path seeds T from
// the enclosing frame, so reflect.type_of<T> resolves correctly (previously
// erased to "unknown").
function forward_reflect<T>() -> string {
    let g = describe<T>;
    g()
}

test "param_dependent_reflect_resolves_T" {
    assert.is_true(baml.deep_equals(forward_reflect<int>(), "int"))
}
