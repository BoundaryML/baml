function Foo() -> int {
  let yyyyyyyy = [1, 2, 3];
  let x = for (i in yyyyyyyy) { i };
  x
}

let x = for (i in "hi") { b };

// error: Error validating: For loop must iterate over a list
//   -->  expr/for_loop.baml:3
//    | 
//  2 |   let yyyyyyyy = [1, 2, 3];
//  3 |   let x = for (i in yyyyyyyy) { i };
//    | 
