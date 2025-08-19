fn EarlyReturn(x: int) -> int {
   if (x == 42) { return 1; }
   
   x + 5
}

fn WithStack(x: int) -> int {
   let a = 1;

   // NOTE: currently there's no empty returns.

   if (a == 0) { return 0; }
   
   {
      let b = 1;
      if (a != b) {
         return 0;
      }
   }
   
   {
      let c = 2;
      let b = 3;
      while (b != c) {
         if (true) {
	          return 0;
	     }
      }
	}

    7
}

fn BadValueReturn(x: int) -> string {
   // should error with `return int, expected string`
   return 1;
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  tests/return.baml:1
//    | 
//    | 
//  1 | fn EarlyReturn(x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  tests/return.baml:7
//    | 
//  6 | 
//  7 | fn WithStack(x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  tests/return.baml:34
//    | 
// 33 | 
// 34 | fn BadValueReturn(x: int) -> string {
//    | 
// error: Expected a string value, but received int value `1`.
//   -->  tests/return.baml:36
//    | 
// 35 |    // should error with `return int, expected string`
// 36 |    return 1;
// 37 | }
//    | 
