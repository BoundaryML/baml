fn AssignWrongType(x: int) -> int {

	let y = 3;

	y = "hello";


	42
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  assign_wrong_type.baml:1
//    | 
//    | 
//  1 | fn AssignWrongType(x: int) -> int {
//    | 
<<<<<<< HEAD
// warning: type for 'y' was inferred here
//   -->  assign_wrong_type.baml:3
//    | 
//  2 | 
//  3 | let y = 3;
//    | 
=======
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  assign_wrong_type.baml:17
//    | 
// 16 | //    | 
// 17 | <<<<<<< HEAD
// 18 | // warning: type for 'y' was inferred here
//    | 
