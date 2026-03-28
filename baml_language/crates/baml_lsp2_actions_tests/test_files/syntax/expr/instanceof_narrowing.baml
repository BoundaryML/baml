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
// Error: unresolved member: user.Bar.field
//     ╭─[ expr_instanceof_narrowing.baml:13:18 ]
//     │
//  13 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── unresolved member: user.Bar.field
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved member: null.field
//     ╭─[ expr_instanceof_narrowing.baml:23:18 ]
//     │
//  23 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── unresolved member: null.field
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved member: user.Bar.field
//     ╭─[ expr_instanceof_narrowing.baml:36:18 ]
//     │
//  36 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── unresolved member: user.Bar.field
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unresolved member: user.Bar.field
//     ╭─[ expr_instanceof_narrowing.baml:38:18 ]
//     │
//  38 │         return x.field;
//     │                  ──┬──  
//     │                    ╰──── unresolved member: user.Bar.field
//     │ 
//     │ Note: Error code: E0001
// ────╯
