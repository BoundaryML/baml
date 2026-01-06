// Test: Nested array type assignment mismatches

class Person {
	name string
	age int
}

class Animal {
	species string
	legs int
}

function NestedArrayAssignments() -> int {
	// === VALID ===
	let a: int[][] = [[1, 2], [3, 4]];
	a = [[5, 6], [7, 8]];

	let b: string[][] = [["a", "b"]];
	b = [["c", "d"]];

	let c: Person[][] = [[Person { name: "A", age: 1 }]];
	c = [[Person { name: "B", age: 2 }]];

	// === INVALID ===

	// int[][] <- int[] (nested vs flat)
	let d: int[][] = [[1, 2]];
	d = [3, 4, 5];

	// int[] <- int[][] (flat vs nested)
	let e = [1, 2, 3];
	e = [[4, 5], [6, 7]];

	// string[][] <- int[][]
	let f: string[][] = [["x", "y"]];
	f = [[1, 2]];

	// Person[][] <- Animal[][]
	let g: Person[][] = [[Person { name: "C", age: 3 }]];
	g = [[Animal { species: "Cat", legs: 4 }]];

	1
}

//----
//- diagnostics
// Error: Expected int[][], found int[]
//     ╭─[ 0:28:5 ]
//     │
//  28 │     d = [3, 4, 5];
//     │        ─────┬────  
//     │             ╰────── Expected int[][], found int[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected int[], found int[][]
//     ╭─[ 0:32:5 ]
//     │
//  32 │     e = [[4, 5], [6, 7]];
//     │        ────────┬────────  
//     │                ╰────────── Expected int[], found int[][]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected string[][], found int[][]
//     ╭─[ 0:36:5 ]
//     │
//  36 │     f = [[1, 2]];
//     │        ────┬────  
//     │            ╰────── Expected string[][], found int[][]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected Person[][], found Animal[][]
//     ╭─[ 0:40:5 ]
//     │
//  40 │     g = [[Animal { species: "Cat", legs: 4 }]];
//     │        ───────────────────┬───────────────────  
//     │                           ╰───────────────────── Expected Person[][], found Animal[][]
//     │ 
//     │ Note: Error code: E0001
// ────╯
