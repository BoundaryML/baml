class InterfaceTwo {
  interface strin
}

class InterfaceOne {
  uses InterfaceT
}

// error: Type `strin` does not exist. Did you mean one of these: `string`, `int`, `float`, `bool`, `InterfaceOne`, `InterfaceTwo`? (error.rs l.499)
//   -->  class/spelling_error.baml:2
//    | 
//  1 | class InterfaceTwo {
//  2 |   interface strin
//    | 
// error: Type strin not found. Did you mean one of these: `string`, `int`, `audio`? (from error.rs l.422)
//   -->  class/spelling_error.baml:2
//    | 
//  1 | class InterfaceTwo {
//  2 |   interface strin
//    | 
// error: Type `InterfaceT` does not exist. Did you mean one of these: `InterfaceTwo`, `InterfaceOne`, `int`, `string`, `float`, `bool`? (error.rs l.499)
//   -->  class/spelling_error.baml:6
//    | 
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    | 
// error: Type InterfaceT not found. Did you mean one of these: `InterfaceTwo`, `InterfaceOne`, `int`? (from error.rs l.422)
//   -->  class/spelling_error.baml:6
//    | 
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    | 
