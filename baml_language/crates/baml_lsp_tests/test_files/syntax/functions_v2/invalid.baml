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
// Error: Expected type, found LBrace
//    ╭─[ 0:1:19 ]
//    │
//  1 │ function Foo() -> {
//    │                   ┬  
//    │                   ╰── Expected type, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found RParen
//    ╭─[ 0:6:20 ]
//    │
//  6 │ function FooBar(arg) -> bar {
//    │                    ┬  
//    │                    ╰── Expected type annotation, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Unknown type bar
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function Foo() -> {
//    │ │ 
//    │ ╰─ Unknown type bar
//    │ 
//    │ Note: Error code: E0002
// ───╯
