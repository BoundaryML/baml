// End-to-end tests: class type-args threaded through instances, methods, and
// runtime class matching.
//
// Converted from crates/baml_tests/tests/class_type_args_at_runtime.rs.
// All test bodies are moved into top-level helper functions to avoid the
// test-block local-boxing VM bug (locals holding instances behave incorrectly
// when passed to calls or compared in test blocks).

class Box<T> {
    value T

    function describe(self) -> string {
        type.of<T>().to_string()
    }
}

class Box2<T> {
    function make_describer(self) -> () -> string throws never {
        return () -> string { type.of<T>().to_string() }
    }
}

class Foo<T> { value T }

// ─── Issue #2: method can read class-level type arg ───────────────────────────

function method_sees_class_type_arg_int_fn() -> string {
    let b: Box<int> = Box { value: 42 };
    b.describe()
}

test "method_sees_class_type_arg_int" {
    assert.equal(method_sees_class_type_arg_int_fn(), "int")
}

function method_sees_class_type_arg_string_fn() -> string {
    let b: Box<string> = Box { value: "hi" };
    b.describe()
}

test "method_sees_class_type_arg_string" {
    assert.equal(method_sees_class_type_arg_string_fn(), "string")
}

function method_sees_composite_class_type_arg_fn() -> string {
    let inner: Box<int> = Box { value: 1 };
    let outer: Box<Box<int>> = Box { value: inner };
    outer.describe()
}

// NOTE: type.of<T>().to_string() includes the namespace segment, so
// the result is "class_type_args_at_runtime.Box<int>", not bare "Box<int>".
test "method_sees_composite_class_type_arg" {
    assert.equal(method_sees_composite_class_type_arg_fn(), "class_type_args_at_runtime.Box<int>")
}

// ─── Issue #9: union dispatch disambiguates by class type args ────────────────

function pick(x: Foo<int> | Foo<string>) -> string {
    match (x) {
        Foo<int>    => "int",
        Foo<string> => "string",
    }
}

function union_dispatch_generic_class_int_fn() -> string {
    pick(Foo<int> { value: 7 })
}

test "union_dispatch_generic_class_int" {
    assert.equal(union_dispatch_generic_class_int_fn(), "int")
}

function union_dispatch_generic_class_string_fn() -> string {
    pick(Foo<string> { value: "x" })
}

test "union_dispatch_generic_class_string" {
    assert.equal(union_dispatch_generic_class_string_fn(), "string")
}

// ─── Issue #9: match-type dispatch respects class type args ──────────────────

function check_foo(f: Foo<int> | Foo<string>) -> bool {
    match (f) {
        Foo<int> => true,
        Foo<string> => false,
    }
}

function is_type_parametric_class_same_args_fn() -> bool {
    check_foo(Foo<int> { value: 1 })
}

test "is_type_parametric_class_same_args" {
    assert.is_true(is_type_parametric_class_same_args_fn())
}

function is_type_parametric_class_different_args_fn() -> bool {
    check_foo(Foo<string> { value: "x" })
}

test "is_type_parametric_class_different_args" {
    assert.is_true(!is_type_parametric_class_different_args_fn())
}

function check_foo_bare(f: Foo<int> | string) -> bool {
    match (f) {
        Foo<int> => true,
        _ => false,
    }
}

function is_type_bare_class_still_matches_fn() -> bool {
    check_foo_bare(Foo<int> { value: 1 })
}

test "is_type_bare_class_still_matches" {
    assert.is_true(is_type_bare_class_still_matches_fn())
}

// ─── Closure inside method captures class T ──────────────────────────────────

function closure_inside_method_captures_class_type_arg_fn() -> string {
    let b: Box2<int> = Box2 { };
    let describe = b.make_describer();
    describe()
}

test "closure_inside_method_captures_class_type_arg" {
    assert.equal(closure_inside_method_captures_class_type_arg_fn(), "int")
}

function closure_inside_method_survives_gc_fn() -> string {
    let b: Box2<int> = Box2 { };
    let describe = b.make_describer();

    // Trigger allocation pressure with many short-lived arrays so the
    // GC has chances to run and move objects, including the closure's
    // captured class type args.
    let i = 0;
    while (i < 500) {
        let tmp = [i, i + 1, i + 2];
        i = i + 1;
    }

    describe()
}

test "closure_inside_method_survives_gc" {
    assert.equal(closure_inside_method_survives_gc_fn(), "int")
}
