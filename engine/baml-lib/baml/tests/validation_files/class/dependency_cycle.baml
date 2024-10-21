// Basic Mutual recursion between two classes.
class InterfaceTwo {
  interface InterfaceOne
}

class InterfaceOne {
  interface InterfaceTwo
}

// Infinite recursion on the same class.
class InterfaceThree {
  interface InterfaceThree
}

// Long cycle.
class One {
  p Two
}

class Two {
  p Three
}

class Three {
  p Four
}

class Four {
  p Five
}

class Five {
  p One
}

// Second independend long cycle.
class A {
  p B
}

class B {
  p C
}

class C {
  p D
}

class D {
  p A
}

// Union that depends on itself.
class Union {
  u Union | Union | (Union | Union)
}

// error: Error validating: These classes form a dependency cycle: InterfaceTwo -> InterfaceOne
//   -->  class/dependency_cycle.baml:2
//    | 
//  1 | // Basic Mutual recursion between two classes.
//  2 | class InterfaceTwo {
//  3 |   interface InterfaceOne
//  4 | }
//    | 
// error: Error validating: These classes form a dependency cycle: InterfaceThree
//   -->  class/dependency_cycle.baml:11
//    | 
// 10 | // Infinite recursion on the same class.
// 11 | class InterfaceThree {
// 12 |   interface InterfaceThree
// 13 | }
//    | 
// error: Error validating: These classes form a dependency cycle: One -> Two -> Three -> Four -> Five
//   -->  class/dependency_cycle.baml:16
//    | 
// 15 | // Long cycle.
// 16 | class One {
// 17 |   p Two
// 18 | }
//    | 
// error: Error validating: These classes form a dependency cycle: A -> B -> C -> D
//   -->  class/dependency_cycle.baml:37
//    | 
// 36 | // Second independend long cycle.
// 37 | class A {
// 38 |   p B
// 39 | }
//    | 
// error: Error validating: These classes form a dependency cycle: Union
//   -->  class/dependency_cycle.baml:54
//    | 
// 53 | // Union that depends on itself.
// 54 | class Union {
// 55 |   u Union | Union | (Union | Union)
// 56 | }
//    | 
