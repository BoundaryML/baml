function assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

function assertNotOk() -> int {
	assert 3 == 1;

	2
}

function assertBool() -> int {

	assert "string";

	1
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `assert`
//    ╭─[assert.baml:3:2]
//  3 │     assert 2 + 2 == 4;
//    ·     ──────
//    ╰────
// E0003
//
//   × unresolved name: `assert`
//    ╭─[assert.baml:9:2]
//  9 │     assert 3 == 1;
//    ·     ──────
//    ╰────
// E0003
//
//   × unresolved name: `assert`
//     ╭─[assert.baml:16:2]
//  16 │     assert "string";
//     ·     ──────
//     ╰────
