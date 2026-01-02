function NoRet(x: int) {
    1
}

// error: function must have a return type: e.g. function Foo() -> int
//   -->  expr/missing_return_value.baml:1
//    | 
//    | 
//  1 | function NoRet(x: int) {
//  2 |     1
//  3 | }
//    |

//----
//- diagnostics
// Error: Expected return type (->), found '{'
//    ╭─[ 0:1:24 ]
//    │
//  1 │ function NoRet(x: int) {
//    │                        ┬  
//    │                        ╰── Expected return type (->), found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
