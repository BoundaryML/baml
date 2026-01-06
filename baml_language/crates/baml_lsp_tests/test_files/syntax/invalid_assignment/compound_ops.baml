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
// Error: Expected `10`, found `5`
//    ╭─[ invalid_assignment_compound_ops.baml:6:7 ]
//    │
//  6 │     a += 5;
//    │          ┬  
//    │          ╰── Expected `10`, found `5`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `10`, found `"five"`
//     ╭─[ invalid_assignment_compound_ops.baml:15:6 ]
//     │
//  15 │     c += "five";
//     │         ───┬───  
//     │            ╰───── Expected `10`, found `"five"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `"start"`, found `100`
//     ╭─[ invalid_assignment_compound_ops.baml:19:7 ]
//     │
//  19 │     d += 100;
//     │          ─┬─  
//     │           ╰─── Expected `"start"`, found `100`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `20`, found `true`
//     ╭─[ invalid_assignment_compound_ops.baml:23:7 ]
//     │
//  23 │     e += true;
//     │          ──┬─  
//     │            ╰─── Expected `20`, found `true`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `true`, found `1`
//     ╭─[ invalid_assignment_compound_ops.baml:27:7 ]
//     │
//  27 │     f += 1;
//     │          ┬  
//     │          ╰── Expected `true`, found `1`
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: Expected `3.5`, found `"add"`
//     ╭─[ invalid_assignment_compound_ops.baml:31:6 ]
//     │
//  31 │     g += "add";
//     │         ───┬──  
//     │            ╰──── Expected `3.5`, found `"add"`
//     │ 
//     │ Note: Error code: E0001
// ────╯
