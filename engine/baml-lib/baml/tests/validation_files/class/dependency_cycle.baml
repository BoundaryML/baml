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
//   -->  class/dependency_cycle.baml:1
//    | 
//    | 
//  1 | class InterfaceTwo {
//    | 
// error: Error validating: These classes form a dependency cycle: InterfaceOne
//   -->  class/dependency_cycle.baml:5
//    | 
//  4 | 
//  5 | class InterfaceOne {
//    | 
// error: Error validating: These classes form a dependency cycle: InterfaceThree
//   -->  class/dependency_cycle.baml:9
//    | 
//  8 | 
//  9 | class InterfaceThree {
//    | 
