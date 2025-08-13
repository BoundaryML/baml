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
         else if y {
            6
         } else {
            7
         };

         a = a + 1;
    }

    3
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  loops/while_loops.baml:14
//    | 
// 13 | 
// 14 | fn Foo() -> int {
// 15 | 
//    | 
