
fn DeclareMutableInFunction(x: int) -> int {

	let mut y = 3;

	y = 5;

	y
}

fn MutableFnArgument(mut x: int) -> int {

	x = 3;
	

	x
}

fn ErrorsForNonMutable(x: int) -> int {

	x = 4;

	42
}
