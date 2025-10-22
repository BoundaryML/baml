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

function WhileWithScopes() -> int {

	while (true) {
		let a = 0;

		// NOTE: seems like nested blocks are not properly parsed?
		// {
		// 	let x = 0;
		// 	if a != x {
		// 		continue;
		// 	}
		// }

		// {
		// 	let y = 0;
		// 	let x = 1;
		// 	if x != y {
		// 		break;
		// 	}
		// }



		if (a > 0) {
			let x = 0; // bogus variable to have different stack lengths @ continue/break
			continue;
		}
		if (a < 0) {
			let y = 0;
			let x = 0;
			break;
		}

		// NOTE: there's a compiler bug wrt PopReplace where it will emit PopReplace when the last block's expression.
		// Avoid that by using a statement. Fixed by #2323.
		a = 1;
	}

	3
}