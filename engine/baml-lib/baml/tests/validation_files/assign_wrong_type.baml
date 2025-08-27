fn AssignWrongType(x: int) -> int {

	let mut y = 3;

	y = "hello";


	42
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  assign_wrong_type.baml:1
//    | 
//    | 
//  1 | fn AssignWrongType(x: int) -> int {
//    | 
// error: Error validating: Cannot assign string to int
//   -->  assign_wrong_type.baml:5
//    | 
//  4 | 
//  5 | y = "hello";
//  6 | 
//    | 
