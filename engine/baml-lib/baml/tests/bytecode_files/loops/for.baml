function Sum(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		result += x;
	}

	result
}

function ForWithBreak(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		if (x > 10) {
			break;
		}
		result += x;
	}

	result
}

function ForWithContinue(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		if (x > 10) {
			continue;
		}
		result += x;
	}

	result
}

function NestedFor(as: int[], bs: int[]) -> int {

	let result = 0;

	for (let a in as) {
		for (let b in bs) {
			result += a * b;
		}
	}

	result
}