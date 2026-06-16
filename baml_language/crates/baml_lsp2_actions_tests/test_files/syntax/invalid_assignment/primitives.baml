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
// error: type mismatch: expected int, got "oops"
//     ╭─[ invalid_assignment_primitives.baml:21:6 ]
//     │
//  21 │     e = "oops";
//     │         ───┬──
//     │            ╰──── type mismatch: expected int, got "oops"
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected string, got 42
//     ╭─[ invalid_assignment_primitives.baml:25:6 ]
//     │
//  25 │     f = 42;
//     │         ─┬
//     │          ╰── type mismatch: expected string, got 42
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected bool, got 123
//     ╭─[ invalid_assignment_primitives.baml:29:6 ]
//     │
//  29 │     g = 123;
//     │         ─┬─
//     │          ╰─── type mismatch: expected bool, got 123
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected int, got false
//     ╭─[ invalid_assignment_primitives.baml:33:6 ]
//     │
//  33 │     h = false;
//     │         ──┬──
//     │           ╰──── type mismatch: expected int, got false
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected float, got "pi"
//     ╭─[ invalid_assignment_primitives.baml:37:6 ]
//     │
//  37 │     i = "pi";
//     │         ──┬─
//     │           ╰─── type mismatch: expected float, got "pi"
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected bool, got "true"
//     ╭─[ invalid_assignment_primitives.baml:41:6 ]
//     │
//  41 │     j = "true";
//     │         ───┬──
//     │            ╰──── type mismatch: expected bool, got "true"
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected string, got true
//     ╭─[ invalid_assignment_primitives.baml:45:6 ]
//     │
//  45 │     k = true;
//     │         ──┬─
//     │           ╰─── type mismatch: expected string, got true
//     │
//     │ Note: Error code: E0001
// ────╯
