class InterfaceTwo {
  interface InterfaceOne
}

class InterfaceOne {
  interface InterfaceTwo
}

class InterfaceThree {
  interface InterfaceThree
}

// error: Error validating: These classes form a dependency cycle: InterfaceTwo
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/class/dependency_cycle.baml:1
//    | 
//    | 
//  1 | class InterfaceTwo {
//  2 |   interface InterfaceOne
//  3 | }
//    | 
// error: Error validating: These classes form a dependency cycle: InterfaceOne
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/class/dependency_cycle.baml:5
//    | 
//  4 | 
//  5 | class InterfaceOne {
//  6 |   interface InterfaceTwo
//  7 | }
//    | 
// error: Error validating: These classes form a dependency cycle: InterfaceThree
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/class/dependency_cycle.baml:9
//    | 
//  8 | 
//  9 | class InterfaceThree {
// 10 |   interface InterfaceThree
// 11 | }
//    | 
