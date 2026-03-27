// Test: Array type assignment mismatches

function ArrayAssignments() -> int {
	// === VALID ===
	let a = [1, 2, 3];
	a = [4, 5, 6];

	let b = ["x", "y"];
	b = ["z", "w"];

	let c = [true, false];
	c = [false, true];

	// === INVALID ===

	// int[] <- string[]
	let d = [1, 2];
	d = ["a", "b"];

	// string[] <- int[]
	let e = ["a", "b"];
	e = [1, 2];

	// int <- int[] (scalar vs array)
	let f = 5;
	f = [1, 2, 3];

	// int[] <- int (array vs scalar)
	let g = [1, 2, 3];
	g = 42;

	// int[] <- string
	let h = [1, 2];
	h = "array";

	// string <- int[]
	let i = "list";
	i = [4, 5, 6];

	1
}

//----
//- diagnostics
// <no-diagnostics-expected>
