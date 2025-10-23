function GCD(a: int, b: int) -> int {

	while (a != b) {
	   if (a > b) {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

// function GCD(a, b) {
//   while a != b {
//   if a > b {
//     a = a - b;
//     } else {
//     b = b - a;
//     } 
//   }
//
//   a
// }
