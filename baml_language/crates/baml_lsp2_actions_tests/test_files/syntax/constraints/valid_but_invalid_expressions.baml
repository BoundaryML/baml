class Foo {
  bar string @check(bar_check, {{ bar }})
}

// warning: Variable `bar` does not exist. Did you mean `this`?
//   -->  constraints/valid_but_invalid_expressions.baml:2
//    |
//  1 | class Foo {
//  2 |   bar string @check(bar_check, {{ bar }})
//    |

//----
//- diagnostics
// warning: Variable `bar` does not exist. Did you mean one of these: `_`, `baml`, `ctx`?
//    ╭─[ constraints_valid_but_invalid_expressions.baml:2:34 ]
//    │
//  2 │   bar string @check(bar_check, {{ bar }})
//    │                                  ─┬─  
//    │                                   ╰─── Variable `bar` does not exist. Did you mean one of these: `_`, `baml`, `ctx`?
//    │ 
//    │ Note: Error code: E0070
// ───╯
