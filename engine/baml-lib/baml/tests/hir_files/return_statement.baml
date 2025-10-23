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

// function EarlyReturn(x) {
//   if x == 42 {
//   return 1;
//   } 
//
//   x + 5
// }
//
// function WithStack(x) {
//   let a = 1;
//   if a == 0 {
//   return 0;
//   } 
//   {
//   let b = 1;
//     if a != b {
//     return 0;
//     } 
//   }
//   {
//   let c = 2;
//     let b = 3;
//     while b != c {
//     if true {
//       return 0;
//       } 
//     }
//   }
//
//   7
// }
