// Test: Compound assignment operator type mismatches

function CompoundAssignments() -> int {
	// === VALID ===
	let a = 10;
	a += 5;

	let b = 1.5;
	b += 2.5;

	// === INVALID ===

	// int += string
	let c = 10;
	c += "five";

	// string += int
	let d = "start";
	d += 100;

	// int += bool
	let e = 20;
	e += true;

	// bool += int
	let f = true;
	f += 1;

	// float += string
	let g = 3.5;
	g += "add";

	1
}

//----
//- diagnostics
// Error: Expected `int`, found `string`
//     ╭─[ invalid_assignment_compound_ops.baml:15:7 ]
//     │
//  15 │     c += "five";
//     │          ───┬──  
//     │             ╰──── Expected `int`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `string`, found `int`
//     ╭─[ invalid_assignment_compound_ops.baml:19:7 ]
//     │
//  19 │     d += 100;
//     │          ─┬─  
//     │           ╰─── Expected `string`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `int`, found `bool`
//     ╭─[ invalid_assignment_compound_ops.baml:23:7 ]
//     │
//  23 │     e += true;
//     │          ──┬─  
//     │            ╰─── Expected `int`, found `bool`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `bool`, found `int`
//     ╭─[ invalid_assignment_compound_ops.baml:27:7 ]
//     │
//  27 │     f += 1;
//     │          ┬  
//     │          ╰── Expected `bool`, found `int`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `float`, found `string`
//     ╭─[ invalid_assignment_compound_ops.baml:31:7 ]
//     │
//  31 │     g += "add";
//     │          ──┬──  
//     │            ╰──── Expected `float`, found `string`
//     │ 
//     │ Note: Error code: E0001
// ────╯
