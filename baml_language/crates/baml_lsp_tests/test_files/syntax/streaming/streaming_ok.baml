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
// Error: Expected Unexpected token in class body, found Dot
//     ╭─[ 0:10:13 ]
//     │
//  10 │     @@stream.done
//     │             ┬  
//     │             ╰── Expected Unexpected token in class body, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type, found RBrace
//     ╭─[ 0:11:1 ]
//     │
//  11 │ }
//     │ ┬  
//     │ ╰── Expected type, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
