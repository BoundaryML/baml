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
