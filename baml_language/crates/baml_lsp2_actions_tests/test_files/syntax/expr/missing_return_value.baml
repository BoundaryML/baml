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
// E0010
//
//   × unexpected token
//    ╭─[expr_missing_return_value.baml:1:24]
//  1 │ function NoRet(x: int) {
//    ·                        ┬
//    ·                        ╰── expected `return type (->)`, found `'{'`
//    ╰────
