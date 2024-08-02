function Foo() -> {
  client Bar
  prompt #"fa"#
}

function FooBar(arg) -> bar {

}

// error: Error validating: No type specified for argument: arg. Expected: `arg: type`
//   -->  functions_v2/invalid.baml:6
//    | 
//  5 | 
//  6 | function FooBar(arg) -> bar {
//    | 
