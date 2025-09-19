function EarlyReturn(x: int) -> int {
   if (x == 42) { return 1; }

   x + 5
}

function WithStack(x: int) -> int {
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

function BadValueReturn(x: int) -> string {
   // should error with `return int, expected string`
   return 1;
}

// error: Expected a string value, but received int value `1`.
//   -->  tests/return.baml:36
//    | 
// 35 |    // should error with `return int, expected string`
// 36 |    return 1;
// 37 | }
//    | 
