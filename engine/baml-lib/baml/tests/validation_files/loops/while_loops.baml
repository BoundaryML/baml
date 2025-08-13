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

fn Foo() -> int {

    let x = true;
    let y = false;

    let mut a = 0;

    while a < 10 {
        let b = if x {
            let c = 5;
            5
         } else if y {
            6
         } else {
            7
         };

         a = a + 1;
    }

    3
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/while_loops.baml:1
//    | 
//    | 
//  1 | fn GCD(mut a: int, mut b: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/while_loops.baml:14
//    | 
// 13 | 
// 14 | fn Foo() -> int {
//    | 
