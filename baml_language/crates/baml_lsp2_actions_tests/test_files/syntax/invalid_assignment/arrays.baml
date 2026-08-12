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
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:18:7]
//  18 │     d = ["a", "b"];
//     ·          ─┬─
//     ·           ╰── expected `int`, found `"a"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:18:12]
//  18 │     d = ["a", "b"];
//     ·               ─┬─
//     ·                ╰── expected `int`, found `"b"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:22:7]
//  22 │     e = [1, 2];
//     ·          ┬
//     ·          ╰── expected `string`, found `1`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:22:10]
//  22 │     e = [1, 2];
//     ·             ┬
//     ·             ╰── expected `string`, found `2`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:26:6]
//  26 │     f = [1, 2, 3];
//     ·         ────┬────
//     ·             ╰── expected `int`, found `int[]`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:30:6]
//  30 │     g = 42;
//     ·         ─┬
//     ·          ╰── expected `int[]`, found `42`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:34:6]
//  34 │     h = "array";
//     ·         ───┬───
//     ·            ╰── expected `int[]`, found `"array"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_arrays.baml:38:6]
//  38 │     i = [4, 5, 6];
//     ·         ────┬────
//     ·             ╰── expected `string`, found `int[]`
//     ╰────
