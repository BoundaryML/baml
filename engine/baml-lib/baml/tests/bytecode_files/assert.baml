fn assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

fn assertNotOk() -> int {
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
// Class: std::Request with 3 fields
// Function: len
