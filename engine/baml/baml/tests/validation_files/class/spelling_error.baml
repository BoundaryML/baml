class InterfaceTwo {
  interface strin
}

class InterfaceOne {
  uses InterfaceT
}




// error: Type `strin` does not exist. Did you mean `string`?
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/class/spelling_error.baml:2
//    | 
//  1 | class InterfaceTwo {
//  2 |   interface strin
//    | 
// error: Type `InterfaceT` does not exist. Did you mean `InterfaceTwo`?
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/class/spelling_error.baml:6
//    | 
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    | 
