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
// error: type mismatch: expected int[], got string[]
//     ╭─[ invalid_assignment_arrays.baml:18:6 ]
//     │
//  18 │     d = ["a", "b"];
//     │         ─────┬────
//     │              ╰────── type mismatch: expected int[], got string[]
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected string[], got int[]
//     ╭─[ invalid_assignment_arrays.baml:22:6 ]
//     │
//  22 │     e = [1, 2];
//     │         ───┬──
//     │            ╰──── type mismatch: expected string[], got int[]
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected int, got int[]
//     ╭─[ invalid_assignment_arrays.baml:26:6 ]
//     │
//  26 │     f = [1, 2, 3];
//     │         ────┬────
//     │             ╰────── type mismatch: expected int, got int[]
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected int[], got 42
//     ╭─[ invalid_assignment_arrays.baml:30:6 ]
//     │
//  30 │     g = 42;
//     │         ─┬
//     │          ╰── type mismatch: expected int[], got 42
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected int[], got "array"
//     ╭─[ invalid_assignment_arrays.baml:34:6 ]
//     │
//  34 │     h = "array";
//     │         ───┬───
//     │            ╰───── type mismatch: expected int[], got "array"
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected string, got int[]
//     ╭─[ invalid_assignment_arrays.baml:38:6 ]
//     │
//  38 │     i = [4, 5, 6];
//     │         ────┬────
//     │             ╰────── type mismatch: expected string, got int[]
//     │
//     │ Note: Error code: E0001
// ────╯
