// Test: Primitive type assignment mismatches

function PrimitiveAssignments() -> int {
	// === VALID ===
	let a = 10;
	a = 20;

	let b = "hello";
	b = "world";

	let c = true;
	c = false;

	let d = 3.14;
	d = 2.71;

	// === INVALID ===

	// int <- string
	let e = 5;
	e = "oops";

	// string <- int
	let f = "text";
	f = 42;

	// bool <- int
	let g = true;
	g = 123;

	// int <- bool
	let h = 10;
	h = false;

	// float <- string
	let i = 1.5;
	i = "pi";

	// bool <- string
	let j = false;
	j = "true";

	// string <- bool
	let k = "flag";
	k = true;

	1
}

//----
//- diagnostics
// <no-diagnostics-expected>
