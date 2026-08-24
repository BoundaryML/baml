// Runtime tests for generic instantiation expressions: `let f = foo<int>`.
//
// `foo<int>` (concrete type args) is a pooled, interned value: identical
// instantiations share one object (pointer-stable identity), and calling the
// value seeds frame.type_args so type-reifying bodies (reflect.Type.of<T>,
// json natives) resolve T at runtime — i.e. it behaves exactly like the
// equivalent direct call `foo<int>(...)`.

function identity<T>(x: T) -> T { x }

// The value runs end-to-end: calling it invokes the underlying function.
function identity_int_value_fn() -> int {
    let f = identity<int>;
    f(5)
}

test "identity_int_value_runs" {
    assert.equal(identity_int_value_fn(), 5)
}

// Two evaluations of `identity<int>` compare equal. NOTE: `==` is
// structural, so this only guards against the value regressing to *inequality*
// — it does not prove the pooled, pointer-stable identity (`foo<int> ===
// foo<int>`). BAML has no in-language identity operator, so that guarantee is
// asserted in a Rust harness that inspects the compiled object pool directly:
// see `tests/instantiation_interning.rs`.
function identity_same_specialization() -> bool {
    let a = identity<int>;
    let b = identity<int>;
    a == b
}

test "same_specialization_is_identical" {
    assert.is_true(identity_same_specialization())
}

// A deep-copied generic-function value compares equal to the original: the
// pooled/interned case is the same object, and `==` also compares non-pooled
// copies structurally (function + type args).
function identity_deep_copy_equal() -> bool {
    let f = identity<int>;
    f == baml.deep_copy(f)
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
    assert.equal(pair_value_fn(), "hello")
}

class User { name string }

function describe<T>() -> string {
    reflect.Type.of<T>().to_string()
}

// reflect.Type.of<T> through an UNCALLED value now resolves T correctly (the
// value carries its type args and seeds frame.type_args when called).
function describe_user_value_fn() -> string {
    let f = describe<User>;
    f()
}

test "reflect_type_of_through_value" {
    assert.equal(describe_user_value_fn(), "instantiation_expr.User")
}

// Type-reifying native through a value: `baml.json.from_string<User>` decodes
// using T=User seeded from the value (previously a runtime crash under erasure).
function from_string_value_fn() -> User {
    let f = baml.json.from_string<User>;
    f("{\"name\":\"Ada\"}")
}

test "json_native_through_value" {
    assert.equal(from_string_value_fn(), User { name: "Ada" })
}

// Param-dependent instantiation (`identity<T>` where T is the enclosing
// function's type param) is built at runtime (MakeGenericFunction resolves the
// type arg against the current frame), so it runs AND reflects on T correctly.
function forward<T>(x: T) -> T {
    let f = identity<T>;
    f(x)
}

test "param_dependent_instantiation_runs" {
    assert.equal(forward<int>(7), 7)
}

// Param-dependent value whose body REFLECTS on T: the runtime path seeds T from
// the enclosing frame, so reflect.Type.of<T> resolves correctly (previously
// erased to "unknown").
function forward_reflect<T>() -> string {
    let g = describe<T>;
    g()
}

test "param_dependent_reflect_resolves_T" {
    assert.equal(forward_reflect<int>(), "int")
}

// Multi-segment qualified instantiation value: a generic function in a nested
// namespace, referenced by its fully-qualified name (`name.space.fun<string>`).
function qualified_value_fn() -> string {
    let f = user.instantiation_expr.qualified.q_describe<string>;
    f()
}
function qualified_value_id_fn() -> int {
    let g = user.instantiation_expr.qualified.q_id<int>;
    g(7)
}

test "qualified_namespaced_value_reflects" {
    assert.equal(qualified_value_fn(), "string")
}
test "qualified_namespaced_value_runs" {
    assert.equal(qualified_value_id_fn(), 7)
}

// A generic instance METHOD referenced as a bound value with explicit type args.
// `p.pick<int>` builds a BoundMethod (carrying the receiver `p`) and specializes
// it; the value cannot be closure-wrapped without losing the receiver, so it is
// passed through with the receiver intact (type args erased — correct for a body
// that does not reify T). (Previously: a runtime crash.)
class GenBox {
    v int
    function pick<T>(self, x: T) -> T { x }
}

function bound_method_value() -> int {
    let p = GenBox { v: 1 };
    let f = p.pick<int>;
    f(7)
}

test "bound_method_value_runs" {
    assert.equal(bound_method_value(), 7)
}

// Companion: a (non-generic) lambda inside a generic function still reifies the
// enclosing function's `U` (no regression from threading lambda params).
function lambda_reflects_enclosing_U<U>() -> string {
    let g = () -> string { reflect.Type.of<U>().to_string() };
    g()
}

test "lambda_reflects_enclosing_U_resolves" {
    assert.equal(lambda_reflects_enclosing_U<int>(), "int")
}
