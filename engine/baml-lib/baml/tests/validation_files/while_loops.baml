fn GCD(mut a: int, mut b: int) -> int {

	while a != b {
	   if a > b {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  while_loops.baml:1
//    | 
//    | 
//  1 | fn GCD(mut a: int, mut b: int) -> int {
//    | 
