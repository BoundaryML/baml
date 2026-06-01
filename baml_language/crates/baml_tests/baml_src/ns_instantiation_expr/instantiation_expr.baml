// Runtime tests for generic instantiation expressions: `let f = foo<int>`.
//
// Type arguments are ERASED at runtime, so an instantiated value behaves like
// the bare callable. For type args that only affect parameter/return types this
// is fully correct (see `identity_int_value_runs`). For a body that reflects on
// its own type parameter it is a known limitation (see the pinned test below) —
// this will be fixed when function mocking reifies the type argument.

function identity<T>(x: T) -> T { x }

// The common case: `<int>` only affects the value's static type. Erasing it at
// runtime is correct — calling the value runs the underlying function.
function identity_int_value_fn() -> int {
    let f = identity<int>;
    f(5)
}

test "identity_int_value_runs" {
    assert.is_true(baml.deep_equals(identity_int_value_fn(), 5))
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

// KNOWN LIMITATION (erasure): an *uncalled* generic reference loses its type
// argument at runtime, so `reflect.type_of<T>` resolves to "unknown" instead of
// the bound type. Pinned so the boundary cannot silently change. Calling
// directly — `describe<User>()` — still returns the bound type (see the
// ns_reflect_type_of_generic fixture).
function describe_user_value_fn() -> string {
    let f = describe<User>;
    f()
}

test "describe_user_value_is_unknown_known_limitation" {
    assert.is_true(baml.deep_equals(describe_user_value_fn(), "unknown"))
}
