function Foo() -> int {
  let x = 5 @emit;
  let y = 10 @emit(skip_def=true);
  let z: int = 20 @emit(when=MyFunction, name=foo_bar);
  10
}


function MyFunction(prev: int, next: int) -> bool {
  true
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:1
//    | 
//    | 
//  1 | function Foo() -> int {
//  2 |   let x = 5 @emit;
//    | 
