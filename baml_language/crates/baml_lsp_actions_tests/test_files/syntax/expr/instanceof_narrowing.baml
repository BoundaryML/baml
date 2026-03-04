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
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:11:9 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │         ────────┬───────  
//     │                 ╰───────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `Foo | Bar` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:13:16 ]
//     │
//  13 │         return x.field;
//     │                ───┬───  
//     │                   ╰───── Type `Foo | Bar` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:35:9 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │         ────────┬───────  
//     │                 ╰───────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `Foo | Bar | Baz` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:36:16 ]
//     │
//  36 │         return x.field;
//     │                ───┬───  
//     │                   ╰───── Type `Foo | Bar | Baz` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:37:16 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                ────────┬───────  
//     │                        ╰───────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `Foo | Bar | Baz` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:38:16 ]
//     │
//  38 │         return x.field;
//     │                ───┬───  
//     │                   ╰───── Type `Foo | Bar | Baz` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     ╭─[ expr_instanceof_narrowing.baml:22:9 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │         ────────┬───────  
//     │                 ╰───────── `instanceof` is no longer supported. Use a `match` expression for type checking instead.
//     │ 
//     │ Note: Error code: E0098
// ────╯
// Error: Type `Foo | null` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:23:16 ]
//     │
//  23 │         return x.field;
//     │                ───┬───  
//     │                   ╰───── Type `Foo | null` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
