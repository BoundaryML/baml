function assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

function assertNotOk() -> int {
	assert 3 == 1;

	2
}

// Function: assertOk
// 2   0   LOAD_CONST 0   (2)
//     1   LOAD_CONST 1   (2)
//     2   BIN_OP +
//     3   LOAD_CONST 2   (4)
//     4   CMP_OP ==
//     5   ASSERT
//
// 4   6   LOAD_CONST 3   (3)
//     7   RETURN
//
// Function: assertNotOk
// 8    0   LOAD_CONST 0   (3)
//      1   LOAD_CONST 1   (1)
//      2   CMP_OP ==
//      3   ASSERT
//
// 10   4   LOAD_CONST 2   (2)
//      5   RETURN
//
<<<<<<< HEAD
// Class: baml.HttpRequest with 5 fields
=======
// Class: baml.Request with 3 fields
// Class: baml.WatchOptions with 2 fields
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:34
//    | 
// 33 | //
// 34 | <<<<<<< HEAD
// 35 | // Class: baml.HttpRequest with 5 fields
//    | 
