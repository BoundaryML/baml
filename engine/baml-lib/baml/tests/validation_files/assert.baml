fn assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

fn assertNotOk() -> int {
	assert 3 == 1;

	2
}

// should yield error
fn assertBool() -> int {

	assert "string";

	1
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  assert.baml:1
//    | 
//    | 
//  1 | fn assertOk() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  assert.baml:8
//    | 
//  7 | 
//  8 | fn assertNotOk() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  assert.baml:15
//    | 
// 14 | // should yield error
// 15 | fn assertBool() -> int {
//    | 
// error: Expected a bool value, but received string value `"string"`.
//   -->  assert.baml:17
//    | 
// 16 | 
// 17 | assert "string";
// 18 | 
//    | 
