class InterfaceThree {
  interface int
  foo
}

// error: Error validating: No type specified for field `foo`
//   -->  class/incomplete_class.baml:3
//    | 
//  2 |   interface int
//  3 |   foo
//  4 | }
//    | 