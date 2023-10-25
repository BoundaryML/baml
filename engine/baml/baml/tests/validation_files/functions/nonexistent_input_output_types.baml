// test comment
function Foo {
  input Bar
  output Baz
}

// error: Type "Bar" is neither a built-in type, nor refers to another model, custom type, or enum.
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/functions/nonexistent_input_output_types.baml:3
//    | 
//  2 | function Foo {
//  3 |   input Bar
//    | 