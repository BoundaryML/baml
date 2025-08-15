fn SumToTen() -> int {
	let mut s = 0;

	for (let mut i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

// should error everywhere `i` is used.
fn NoIteratorDeclared() -> int {
	let mut s = 0;

	for (; i <= 10; i += 1) {
		s += i;
	}

	s
}

// should be fine
fn LoopHeaderScope() -> int {
	let mut s = 0;

	for (let mut i = 0; i <= 10; s += i) {
	}

	s
}

// should error in `i += x` (no `x` in scope)
fn LoopHeaderScopeUnavailable() -> int {
	let mut s = 0;

	for (let mut i = 0; i <= 10; i += x) {
		let x = 1;
	}

	s
}
