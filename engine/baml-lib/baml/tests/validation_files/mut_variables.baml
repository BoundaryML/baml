fn DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

fn MutableInArg(x: int) -> int {
	x = 3;
	x
}

fn ErrorWhenAssigningToConst(x: int) -> int {
	x = 42;
	x
}