function DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

function MutableInArg(x: int) -> int {
	x = 3;
	x
}

// Function: DeclareMutableInFunction
// 2   0   LOAD_CONST 0   (3)
//
// 4   1   LOAD_CONST 1   (5)
//     2   STORE_VAR 2    (y)
//
// 6   3   LOAD_VAR 2     (y)
//     4   RETURN         
//
// Function: MutableInArg
// 10   0   LOAD_CONST 0   (3)
//      1   STORE_VAR 1    (x)
//
// 11   2   LOAD_VAR 1     (x)
//      3   RETURN         
//
<<<<<<< HEAD
// Class: baml.HttpRequest with 5 fields
=======
// Class: baml.Request with 3 fields
// Class: baml.WatchOptions with 2 fields
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:31
//    | 
// 30 | //
// 31 | <<<<<<< HEAD
// 32 | // Class: baml.HttpRequest with 5 fields
//    | 
