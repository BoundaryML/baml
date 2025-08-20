fn While() -> int {
	while (true) {}

	while (true {
	   let i = 0; // TODO: for some reason empty loop without both parens does not parse with PEST
	}

	while true) {
	   let i = 0; // TODO: for some reason empty loop without both parens does not parse with PEST
	}

        while true {
           let i = 0; // TODO: for some reason empty loop without both parens does not parse with PEST
        }
}


fn ForIterator(xs: int[]) -> int {
	for (x in xs) {}
	for (x in xs {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}
	for x in xs) {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}
	for x in xs {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}
}

fn ForRegular() -> int {
	for (let mut i = 0; i < 3; i += 1) {}

	for let mut i = 0; i < 3; i += 1) {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}

	for (let mut i = 0; i < 3; i += 1 {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}

	for let mut i = 0; i < 3; i += 1 {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}
}

fn If() -> int {
	if (true) {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}

	if true) {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}

	if (true {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}

	if true {
		let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
	}
}

fn FnCall() -> int {
	If()
}
