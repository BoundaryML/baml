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

