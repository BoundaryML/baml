function SumToTen() -> int {
	let s = 0;

	for (let i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

function AfterWithBreakContinue() -> int {
	let s = 0;

	for (let i = 0; ; s += i) {
		i += 1;
		if (i > 10) {
			break;
		}
		if (i == 5) {
			continue;
		}
	}

	s
}


function OnlyCond() -> int {
	let s = 0;

	for (; false;) {
	}

	s
}


function Nothing() -> int {
	let s = 0;

	for (;;) {
		break;
	}

	s
}