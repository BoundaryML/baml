fn Sum(xs: int[]) -> int {
	let mut result = 0;

	for (x in xs) {
		result += x;
	}

	result
}

fn SumBadType(xs: string[]) -> int {
	let mut result = 0;

	for (x in xs) {
		// should complain about int vs. string.
		result = x;
	}

	result
}

// NOTE: `let mut b: string = ""` is not supported yet, so the variable is declared
// as an argument instead.
fn SumUnknownType(xs: int, mut b: string) -> int {


	// this should error in `xs` saying it's not an array.
	for (x in xs) {

		// this one will error with "cannot assign string to int"...
		b = x;
		// ...which is expected behavior for now. See thir/typecheck.rs's ForLoop
		// typecheck for more info.

	}

	0
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  loops/for.baml:1
//    | 
//    | 
//  1 | fn Sum(xs: int[]) -> int {
//  2 | let mut result = 0;
//    | 
