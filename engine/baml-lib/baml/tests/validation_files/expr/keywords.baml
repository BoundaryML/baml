function Foo(emit: string) -> int {
  1
}

// error: Error validating: 'emit' is a reserved keyword.
//   -->  expr/keywords.baml:1
//    | 
//    | 
//  1 | function Foo(emit: string) -> int {
//  2 |   1
//  3 | }
//    | 
