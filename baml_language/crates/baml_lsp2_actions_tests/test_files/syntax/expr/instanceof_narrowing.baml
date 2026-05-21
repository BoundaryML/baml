// Test 1: Simple instanceof narrowing
class Foo {
    field string
}

class Bar {
    other int
}

function test_simple_narrowing(x: Foo | Bar) -> string {
    if (x instanceof Foo) {
        // Should be able to access Foo.field here
        return x.field;
    } else {
        // x is still Foo | Bar here (or could be narrowed to Bar)
        return "not foo";
    }
}

// Test 2: Union with null
function test_optional_narrowing(x: Foo | null) -> string {
    if (x instanceof Foo) {
        return x.field;
    } else {
        return "null";
    }
}

// Test 3: Multiple classes in union
class Baz {
    field string  // Same field name as Foo
}

function test_multiple_classes(x: Foo | Bar | Baz) -> string {
    if (x instanceof Foo) {
        return x.field;
    } else if (x instanceof Baz) {
        return x.field;
    } else {
        return "bar";
    }
}

//----
//- diagnostics
// error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:11:11 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── use `match` instead
//     │ 
//     │ Note: Error code: E0098
// ────╯
// error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:22:11 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── use `match` instead
//     │ 
//     │ Note: Error code: E0098
// ────╯
// error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:35:11 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── use `match` instead
//     │ 
//     │ Note: Error code: E0098
// ────╯
// error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:37:18 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                  ─────┬────  
//     │                       ╰────── use `match` instead
//     │ 
//     │ Note: Error code: E0098
// ────╯
// error: type `user.Bar` has no member `field`
//     ╭─[ expr_instanceof_narrowing.baml:13:18 ]
//     │
//  13 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── type `user.Bar` has no member `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// error: did you mean `x?.field`? `x.field` does not handle the case when `x` is null
//     ╭─[ expr_instanceof_narrowing.baml:23:15 ]
//     │
//  23 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── did you mean `x?.field`? `x.field` does not handle the case when `x` is null
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected string, got string?
//     ╭─[ expr_instanceof_narrowing.baml:23:15 ]
//     │
//  23 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── type mismatch: expected string, got string?
//     │ 
//     │ Note: Error code: E0001
// ────╯
// error: type `user.Bar` has no member `field`
//     ╭─[ expr_instanceof_narrowing.baml:36:18 ]
//     │
//  36 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── type `user.Bar` has no member `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// error: type `user.Bar` has no member `field`
//     ╭─[ expr_instanceof_narrowing.baml:38:18 ]
//     │
//  38 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── type `user.Bar` has no member `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
