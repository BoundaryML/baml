fn Sum(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		result += x;
	}

	result
}

fn ForWithBreak(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		if x > 10 {
			break;
		}
		result += x;
	}

	result
}

fn ForWithContinue(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		if x > 10 {
			continue;
		}
		result += x;
	}

	result
}

fn NestedFor(as: int[], bs: int[]) -> int {

	let mut result = 0;

	for a in as {
		for b in bs {
			result += a * b;
		}
	}

	result
}
