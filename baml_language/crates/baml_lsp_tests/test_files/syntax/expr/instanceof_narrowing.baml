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
// Error: Expected RParen, found Instanceof
//     ╭─[ 0:11:11 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected RParen, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found Instanceof
//     ╭─[ 0:11:11 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected block after if condition, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Instanceof
//     ╭─[ 0:11:11 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected expression, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:11:25 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │                         ┬  
//     │                         ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Else
//     ╭─[ 0:14:7 ]
//     │
//  14 │     } else {
//     │       ──┬─  
//     │         ╰─── Expected expression, found Else
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Instanceof
//     ╭─[ 0:22:11 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected RParen, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found Instanceof
//     ╭─[ 0:22:11 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected block after if condition, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Instanceof
//     ╭─[ 0:22:11 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected expression, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:22:25 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │                         ┬  
//     │                         ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Else
//     ╭─[ 0:24:7 ]
//     │
//  24 │     } else {
//     │       ──┬─  
//     │         ╰─── Expected expression, found Else
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Instanceof
//     ╭─[ 0:35:11 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected RParen, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found Instanceof
//     ╭─[ 0:35:11 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected block after if condition, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Instanceof
//     ╭─[ 0:35:11 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │           ─────┬────  
//     │                ╰────── Expected expression, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:35:25 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │                         ┬  
//     │                         ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Else
//     ╭─[ 0:37:7 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │       ──┬─  
//     │         ╰─── Expected expression, found Else
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Instanceof
//     ╭─[ 0:37:18 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                  ─────┬────  
//     │                       ╰────── Expected RParen, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected block after if condition, found Instanceof
//     ╭─[ 0:37:18 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                  ─────┬────  
//     │                       ╰────── Expected block after if condition, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Instanceof
//     ╭─[ 0:37:18 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                  ─────┬────  
//     │                       ╰────── Expected expression, found Instanceof
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found RParen
//     ╭─[ 0:37:32 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                                ┬  
//     │                                ╰── Expected expression, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected expression, found Else
//     ╭─[ 0:39:7 ]
//     │
//  39 │     } else {
//     │       ──┬─  
//     │         ╰─── Expected expression, found Else
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Unknown variable Foo
//     ╭─[ 0:11:22 ]
//     │
//  11 │     if (x instanceof Foo) {
//     │                      ─┬─  
//     │                       ╰─── Unknown variable Foo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type Foo | Bar has no field 'field'
//     ╭─[ 0:13:15 ]
//     │
//  13 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type Foo | Bar has no field 'field'
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Unknown variable Foo
//     ╭─[ 0:35:22 ]
//     │
//  35 │     if (x instanceof Foo) {
//     │                      ─┬─  
//     │                       ╰─── Unknown variable Foo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type Foo | Bar | Baz has no field 'field'
//     ╭─[ 0:36:15 ]
//     │
//  36 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type Foo | Bar | Baz has no field 'field'
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Unknown variable Baz
//     ╭─[ 0:37:29 ]
//     │
//  37 │     } else if (x instanceof Baz) {
//     │                             ─┬─  
//     │                              ╰─── Unknown variable Baz
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type Foo | Bar | Baz has no field 'field'
//     ╭─[ 0:38:15 ]
//     │
//  38 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type Foo | Bar | Baz has no field 'field'
//     │ 
//     │ Note: Error code: E0007
// ────╯
// Error: Unknown variable Foo
//     ╭─[ 0:22:22 ]
//     │
//  22 │     if (x instanceof Foo) {
//     │                      ─┬─  
//     │                       ╰─── Unknown variable Foo
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Type Foo | null has no field 'field'
//     ╭─[ 0:23:15 ]
//     │
//  23 │         return x.field;
//     │               ────┬───  
//     │                   ╰───── Type Foo | null has no field 'field'
//     │ 
//     │ Note: Error code: E0007
// ────╯
