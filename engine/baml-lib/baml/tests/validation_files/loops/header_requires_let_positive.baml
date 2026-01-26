function OuterDeclared() -> int {
	let i = 100;

	for (i = 0; i < 3; i += 1) {
	}

	0
}

function InitWithLet() -> int {
	let s = 0;

	for (let j = 0; j < 3; j += 1) {
		s += j;
	}

	s
}

