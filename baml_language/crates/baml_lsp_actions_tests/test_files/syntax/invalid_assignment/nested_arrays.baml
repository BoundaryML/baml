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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: type mismatch: expected int[][], got 3 | 4 | 5[]
//     ╭─[ invalid_assignment_nested_arrays.baml:28:5 ]
//     │
//  28 │     d = [3, 4, 5];
//     │        ─────┬────  
//     │             ╰────── type mismatch: expected int[][], got 3 | 4 | 5[]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: type mismatch: expected string[][], got 1 | 2[][]
//     ╭─[ invalid_assignment_nested_arrays.baml:36:5 ]
//     │
//  36 │     f = [[1, 2]];
//     │        ────┬────  
//     │            ╰────── type mismatch: expected string[][], got 1 | 2[][]
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: type mismatch: expected user.Person[][], got user.Animal[][]
//     ╭─[ invalid_assignment_nested_arrays.baml:40:5 ]
//     │
//  40 │     g = [[Animal { species: "Cat", legs: 4 }]];
//     │        ───────────────────┬───────────────────  
//     │                           ╰───────────────────── type mismatch: expected user.Person[][], got user.Animal[][]
//     │ 
//     │ Note: Error code: E0001
// ────╯
