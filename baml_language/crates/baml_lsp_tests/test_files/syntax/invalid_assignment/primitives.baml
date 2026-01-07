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
// Error: Expected `int`, found `string`
//     ╭─[ invalid_assignment_primitives.baml:21:5 ]
//     │
//  21 │     e = "oops";
//     │        ───┬───  
//     │           ╰───── Expected `int`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `string`, found `int`
//     ╭─[ invalid_assignment_primitives.baml:25:6 ]
//     │
//  25 │     f = 42;
//     │         ─┬  
//     │          ╰── Expected `string`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `bool`, found `int`
//     ╭─[ invalid_assignment_primitives.baml:29:6 ]
//     │
//  29 │     g = 123;
//     │         ─┬─  
//     │          ╰─── Expected `bool`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int`, found `bool`
//     ╭─[ invalid_assignment_primitives.baml:33:6 ]
//     │
//  33 │     h = false;
//     │         ──┬──  
//     │           ╰──── Expected `int`, found `bool`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `float`, found `string`
//     ╭─[ invalid_assignment_primitives.baml:37:5 ]
//     │
//  37 │     i = "pi";
//     │        ──┬──  
//     │          ╰──── Expected `float`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `bool`, found `string`
//     ╭─[ invalid_assignment_primitives.baml:41:5 ]
//     │
//  41 │     j = "true";
//     │        ───┬───  
//     │           ╰───── Expected `bool`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `string`, found `bool`
//     ╭─[ invalid_assignment_primitives.baml:45:6 ]
//     │
//  45 │     k = true;
//     │         ──┬─  
//     │           ╰─── Expected `string`, found `bool`
//     │ 
//     │ Note: Error code: E0001
// ────╯
