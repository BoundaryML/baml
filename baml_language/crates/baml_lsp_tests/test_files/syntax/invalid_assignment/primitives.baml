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
// Error: Expected `10`, found `20`
//    ╭─[ invalid_assignment_primitives.baml:6:6 ]
//    │
//  6 │     a = 20;
//    │         ─┬  
//    │          ╰── Expected `10`, found `20`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `"hello"`, found `"world"`
//    ╭─[ invalid_assignment_primitives.baml:9:5 ]
//    │
//  9 │     b = "world";
//    │        ────┬───  
//    │            ╰───── Expected `"hello"`, found `"world"`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `true`, found `false`
//     ╭─[ invalid_assignment_primitives.baml:12:6 ]
//     │
//  12 │     c = false;
//     │         ──┬──  
//     │           ╰──── Expected `true`, found `false`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `5`, found `"oops"`
//     ╭─[ invalid_assignment_primitives.baml:21:5 ]
//     │
//  21 │     e = "oops";
//     │        ───┬───  
//     │           ╰───── Expected `5`, found `"oops"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `"text"`, found `42`
//     ╭─[ invalid_assignment_primitives.baml:25:6 ]
//     │
//  25 │     f = 42;
//     │         ─┬  
//     │          ╰── Expected `"text"`, found `42`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `true`, found `123`
//     ╭─[ invalid_assignment_primitives.baml:29:6 ]
//     │
//  29 │     g = 123;
//     │         ─┬─  
//     │          ╰─── Expected `true`, found `123`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `10`, found `false`
//     ╭─[ invalid_assignment_primitives.baml:33:6 ]
//     │
//  33 │     h = false;
//     │         ──┬──  
//     │           ╰──── Expected `10`, found `false`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `1.5`, found `"pi"`
//     ╭─[ invalid_assignment_primitives.baml:37:5 ]
//     │
//  37 │     i = "pi";
//     │        ──┬──  
//     │          ╰──── Expected `1.5`, found `"pi"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `false`, found `"true"`
//     ╭─[ invalid_assignment_primitives.baml:41:5 ]
//     │
//  41 │     j = "true";
//     │        ───┬───  
//     │           ╰───── Expected `false`, found `"true"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `"flag"`, found `true`
//     ╭─[ invalid_assignment_primitives.baml:45:6 ]
//     │
//  45 │     k = true;
//     │         ──┬─  
//     │           ╰─── Expected `"flag"`, found `true`
//     │ 
//     │ Note: Error code: E0001
// ────╯
