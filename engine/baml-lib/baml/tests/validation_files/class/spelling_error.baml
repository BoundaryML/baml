class InterfaceTwo {
  interface strin
}

class InterfaceOne {
  uses InterfaceT
}






// error: Type `strin` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `InterfaceOne`, `InterfaceTwo`?
//   -->  class/spelling_error.baml:2
//    | 
//  1 | class InterfaceTwo {
//  2 |   interface strin
//    | 
// error: Type `InterfaceT` does not exist. Did you mean one of these: `InterfaceTwo`, `InterfaceOne`, `int`, `string`, `float`, `bool`?
//   -->  class/spelling_error.baml:6
//    | 
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    | 
