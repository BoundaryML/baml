function Sum(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		result += x;
	}

	result
}

function SumBadType(xs: string[]) -> int {
	let result = 0;

	for (let x in xs) {
		// should complain about int vs. string.
		result = x;
	}

	result
}

// NOTE: `let mut b: string = ""` is not supported yet, so the variable is declared
// as an argument instead.
function SumUnknownType(xs: int, b: string) -> int {


	// this should error in `xs` saying it's not an array.
	for (let x in xs) {

		// this one will error with "cannot assign string to int"...
		b = x;
		// ...which is expected behavior for now. See thir/typecheck.rs's ForLoop
		// typecheck for more info.

	}

	0
}

// error: Error validating: Cannot assign string to int
//   -->  loops/for.baml:16
//    | 
// 15 | // should complain about int vs. string.
// 16 | result = x;
// 17 | }
//    | 
// error: Error validating: iterable in `for` loop must be an array
//   -->  loops/for.baml:28
//    | 
// 27 | // this should error in `xs` saying it's not an array.
// 28 | for (let x in xs) {
//    | 
// error: Error validating: Cannot assign int to string
//   -->  loops/for.baml:31
//    | 
// 30 | // this one will error with "cannot assign string to int"...
// 31 | b = x;
// 32 | // ...which is expected behavior for now. See thir/typecheck.rs's ForLoop
//    | 
