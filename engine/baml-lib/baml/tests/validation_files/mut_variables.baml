function DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

function MutableInArg(x: int) -> int {
	x = 3;
	x
}

function ErrorWhenAssigningToConst(x: int) -> int {
	x = 42;
	x
}
