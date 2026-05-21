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
// error: operator `Add` cannot be applied to `int` and `true`
//     ╭─[ invalid_assignment_compound_ops.baml:23:2 ]
//     │
//  23 │     e += true;
//     │     ┬  
//     │     ╰── operator `Add` cannot be applied to `int` and `true`
//     │ 
//     │ Note: Error code: E0004
// ────╯
// error: operator `Add` cannot be applied to `bool` and `1`
//     ╭─[ invalid_assignment_compound_ops.baml:27:2 ]
//     │
//  27 │     f += 1;
//     │     ┬  
//     │     ╰── operator `Add` cannot be applied to `bool` and `1`
//     │ 
//     │ Note: Error code: E0004
// ────╯
