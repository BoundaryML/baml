function assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

function assertNotOk() -> int {
	assert 3 == 1;

	2
}

// function assertOk() {
//   assert 2 + 2 == 4;
//
//   3
// }
//
// function assertNotOk() {
//   assert 3 == 1;
//
//   2
// }
