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
// E0004
//
//   × operator `+` cannot be applied to `int` and `"five"`
//     ╭─[invalid_assignment_compound_ops.baml:15:7]
//  15 │     c += "five";
//     ·          ──────
//     ╰────
// E0004
//
//   × operator `+` cannot be applied to `string` and `100`
//     ╭─[invalid_assignment_compound_ops.baml:19:7]
//  19 │     d += 100;
//     ·          ───
//     ╰────
// E0004
//
//   × operator `+` cannot be applied to `int` and `true`
//     ╭─[invalid_assignment_compound_ops.baml:23:7]
//  23 │     e += true;
//     ·          ────
//     ╰────
// E0004
//
//   × operator `+` cannot be applied to `bool` and `1`
//     ╭─[invalid_assignment_compound_ops.baml:27:7]
//  27 │     f += 1;
//     ·          ─
//     ╰────
// E0004
//
//   × operator `+` cannot be applied to `float` and `"add"`
//     ╭─[invalid_assignment_compound_ops.baml:31:7]
//  31 │     g += "add";
//     ·          ─────
//     ╰────
