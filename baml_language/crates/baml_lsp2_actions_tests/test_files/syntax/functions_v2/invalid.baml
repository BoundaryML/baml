function Foo() -> {
  client Bar
  prompt #"fa"#
}

function FooBar(arg) -> bar {

}

// error: Error validating value expression "Foo": No return type specified. Valid function syntax is
// ```
// function Foo(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/invalid.baml:1
//    |
//    |
//  1 | function Foo() -> {
//  2 |   client Bar
//  3 |   prompt #"fa"#
//  4 | }
//    |
// error: Error validating: No type specified for argument: arg. Expected: `arg: type`
//   -->  functions_v2/invalid.baml:6
//    |
//  5 |
//  6 | function FooBar(arg) -> bar {
//    |

//----
//- diagnostics
// error: Expected type, found '{'
//    ╭─[ functions_v2_invalid.baml:1:19 ]
//    │
//  1 │ function Foo() -> {
//    │                   ┬
//    │                   ╰── Expected type, found '{'
//    │
//    │ Note: Error code: E0010
// ───╯
// error: Expected type annotation, found ')'
//    ╭─[ functions_v2_invalid.baml:6:20 ]
//    │
//  6 │ function FooBar(arg) -> bar {
//    │                    ┬
//    │                    ╰── Expected type annotation, found ')'
//    │
//    │ Note: Error code: E0010
// ───╯
// error: could not parse type expression for return type of `Foo`
//    ╭─[ functions_v2_invalid.baml:1:18 ]
//    │
//  1 │ function Foo() -> {
//    │                  │
//    │                  ╰─ unparseable type
//    │
//    │ Note: Error code: E0108
// ───╯
