fn DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

fn MutableInArg(mut x: int) -> int {
	x = 3;
	x
}

fn ErrorWhenAssigningToConst(x: int) -> int {
	x = 42;
	x
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  mut_variables.baml:10
//    | 
//  9 | 
// 10 | fn MutableInArg(mut x: int) -> int {
// 11 | x = 3;
//    | 
