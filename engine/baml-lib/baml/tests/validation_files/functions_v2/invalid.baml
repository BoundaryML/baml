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
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  functions_v2/invalid.baml:6
//    | 
//  5 | 
//  6 | function FooBar(arg) -> bar {
//    | 
