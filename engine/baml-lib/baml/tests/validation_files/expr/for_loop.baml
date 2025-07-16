function Foo() -> int[] {
  let list = [1, 2, 3];
  for (i in list) { i + 1 }

  list
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/for_loop.baml:1
//    | 
//    | 
//  1 | function Foo() -> int[] {
//    | 
