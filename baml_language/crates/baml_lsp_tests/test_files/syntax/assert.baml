function assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

function assertNotOk() -> int {
	assert 3 == 1;

	2
}

// should yield error
function assertBool() -> int {

	assert "string";

	1
}

// error: Expected a bool value, but received string value `"string"`.
//   -->  assert.baml:17
//    | 
// 16 | 
// 17 | assert "string";
// 18 | 
//    |

//----
//- diagnostics
// Error: Unknown variable assert
//    ╭─[ 0:9:2 ]
//    │
//  9 │     assert 3 == 1;
//    │     ───┬──  
//    │        ╰──── Unknown variable assert
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable assert
//    ╭─[ 0:3:2 ]
//    │
//  3 │     assert 2 + 2 == 4;
//    │     ───┬──  
//    │        ╰──── Unknown variable assert
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable assert
//     ╭─[ 0:17:2 ]
//     │
//  17 │     assert "string";
//     │     ───┬──  
//     │        ╰──── Unknown variable assert
//     │ 
//     │ Note: Error code: E0003
// ────╯
