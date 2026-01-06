class Foo {
    bar int @stream.not_null
    baz string | int @stream.done
    quux bool @stream.with_state
}

class Foo2 {
    something int
    tomething_else bool
    @@stream.done
}

//----
//- diagnostics
// Error: Expected Unexpected token in class body, found '.'
//     ╭─[ 0:10:13 ]
//     │
//  10 │     @@stream.done
//     │             ┬  
//     │             ╰── Expected Unexpected token in class body, found '.'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: field 'done' is missing a type annotation
//     ╭─[ 0:10:14 ]
//     │
//  10 │     @@stream.done
//     │              ──┬─  
//     │                ╰─── field 'done' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
