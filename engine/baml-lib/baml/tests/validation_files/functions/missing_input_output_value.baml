function A {
  input 
}

function B {
  input string
  output
}

// error: Error validating: Missing function field type
//   -->  functions/missing_input_output_value.baml:2
//    | 
//  1 | function A {
//  2 |   input 
//  3 | }
//    | 
// error: Error validating function "A": This function declaration is invalid. It is missing an input or output field.
//   -->  functions/missing_input_output_value.baml:1
//    | 
//    | 
//  1 | function A {
//  2 |   input 
//  3 | }
//    | 
// error: Error validating: Missing function field type
//   -->  functions/missing_input_output_value.baml:7
//    | 
//  6 |   input string
//  7 |   output
//  8 | }
//    | 
// error: Error validating function "B": This function declaration is invalid. It is missing an input or output field.
//   -->  functions/missing_input_output_value.baml:5
//    | 
//  4 | 
//  5 | function B {
//  6 |   input string
//  7 |   output
//  8 | }
//    | 
