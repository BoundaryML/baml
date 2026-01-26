function DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

function MutableInArg(x: int) -> int {
	x = 3;
	x
}

// function DeclareMutableInFunction(x) {
//   let y = 3;
//   y = 5;
//
//   y
// }
//
// function MutableInArg(x) {
//   x = 3;
//
//   x
// }
