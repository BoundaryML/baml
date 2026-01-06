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
// Error: Expected `int[]`, found `string[]`
//     ╭─[ invalid_assignment_arrays.baml:18:5 ]
//     │
//  18 │     d = ["a", "b"];
//     │        ─────┬─────  
//     │             ╰─────── Expected `int[]`, found `string[]`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `string[]`, found `int[]`
//     ╭─[ invalid_assignment_arrays.baml:22:5 ]
//     │
//  22 │     e = [1, 2];
//     │        ───┬───  
//     │           ╰───── Expected `string[]`, found `int[]`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `5`, found `int[]`
//     ╭─[ invalid_assignment_arrays.baml:26:5 ]
//     │
//  26 │     f = [1, 2, 3];
//     │        ─────┬────  
//     │             ╰────── Expected `5`, found `int[]`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int[]`, found `42`
//     ╭─[ invalid_assignment_arrays.baml:30:6 ]
//     │
//  30 │     g = 42;
//     │         ─┬  
//     │          ╰── Expected `int[]`, found `42`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int[]`, found `"array"`
//     ╭─[ invalid_assignment_arrays.baml:34:5 ]
//     │
//  34 │     h = "array";
//     │        ────┬───  
//     │            ╰───── Expected `int[]`, found `"array"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `"list"`, found `int[]`
//     ╭─[ invalid_assignment_arrays.baml:38:5 ]
//     │
//  38 │     i = [4, 5, 6];
//     │        ─────┬────  
//     │             ╰────── Expected `"list"`, found `int[]`
//     │ 
//     │ Note: Error code: E0001
// ────╯
