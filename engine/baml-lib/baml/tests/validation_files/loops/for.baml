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

// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/for.baml:1
//    | 
//    | 
//  1 | fn Sum(xs: int[]) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/for.baml:11
//    | 
// 10 | 
// 11 | fn SumBadType(xs: string[]) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/for.baml:24
//    | 
// 23 | // as an argument instead.
// 24 | fn SumUnknownType(xs: int, mut b: string) -> int {
//    | 
// warning: type for 'result' was inferred here
//   -->  loops/for.baml:12
//    | 
// 11 | fn SumBadType(xs: string[]) -> int {
// 12 | let mut result = 0;
//    | 
// warning: type for 'b' was inferred here
//   -->  loops/for.baml:24
//    | 
// 23 | // as an argument instead.
// 24 | fn SumUnknownType(xs: int, mut b: string) -> int {
//    | 
// error: Error validating: Cannot assign string to int
//   -->  loops/for.baml:16
//    | 
// 15 | // should complain about int vs. string.
// 16 | result = x;
//    | 
// error: Error validating: iterable in `for` loop must be an array
//   -->  loops/for.baml:28
//    | 
// 27 | // this should error in `xs` saying it's not an array.
// 28 | for (x in xs) {
//    | 
// error: Error validating: Cannot assign int to string
//   -->  loops/for.baml:31
//    | 
// 30 | // this one will error with "cannot assign string to int"...
// 31 | b = x;
//    | 
