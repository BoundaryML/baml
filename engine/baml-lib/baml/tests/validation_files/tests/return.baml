fn EarlyReturn(x: int) -> int {
   if x == 42 { return 1; }
   
   x + 5
}

fn WithStack(x: int) -> null {
   let a = 1;

   // both 'return' and 'return null' should be allowed if context allows it.

   if a == 0 { return; }
   
   {
      let b = 1;
      if a != b {
         return;
      }
   }
   
   {
      let c = 2;
      let b = 3;
      while b != c {
         if true {
	          return null;
	     }
      }
	}
}

fn BadVoidReturn(x: int) -> int {

   // should error with 'return null, expected int`
   return;
}

fn BadValueReturn(x: int) -> string {
   // should error with `return int, expected string`
   return 1;
}
