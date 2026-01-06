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
// Error: Type `Foo | Bar` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:13:15 ]
//     │
//  13 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type `Foo | Bar` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Foo | Bar | Baz` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:36:15 ]
//     │
//  36 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type `Foo | Bar | Baz` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Foo | Bar | Baz` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:38:15 ]
//     │
//  38 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type `Foo | Bar | Baz` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Type `Foo | null` has no field `field`
//     ╭─[ expr_instanceof_narrowing.baml:23:15 ]
//     │
//  23 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type `Foo | null` has no field `field`
//     │ 
//     │ Note: Error code: E0007
// ────╯
