fn DeclareMutableInFunction(x: int) -> int {

	let mut y = 3;

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

// warning: Workflow functions are experimental, and will break in the future.
//   -->  mut_variables.baml:1
//    | 
//    | 
//  1 | fn DeclareMutableInFunction(x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  mut_variables.baml:10
//    | 
//  9 | 
// 10 | fn MutableInArg(mut x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  mut_variables.baml:15
//    | 
// 14 | 
// 15 | fn ErrorWhenAssigningToConst(x: int) -> int {
//    | 
// error: Error validating: Cannot assign to immutable variable x
//   -->  mut_variables.baml:16
//    | 
// 15 | fn ErrorWhenAssigningToConst(x: int) -> int {
// 16 | x = 42;
//    | 
