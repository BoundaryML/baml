// test comment
function Foo {
  input Bar
  output Baz
}




// error: Type `Bar` does not exist.
//   -->  functions/nonexistent_input_output_types.baml:3
//    | 
//  2 | function Foo {
//  3 |   input Bar
//    | 
// error: Type `Baz` does not exist.
//   -->  functions/nonexistent_input_output_types.baml:4
//    | 
//  3 |   input Bar
//  4 |   output Baz
//    | 
