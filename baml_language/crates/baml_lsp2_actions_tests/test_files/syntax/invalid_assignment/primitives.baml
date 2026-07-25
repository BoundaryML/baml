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
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:21:6]
//  21 │     e = "oops";
//     ·         ───┬──
//     ·            ╰── expected `int`, found `"oops"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:25:6]
//  25 │     f = 42;
//     ·         ─┬
//     ·          ╰── expected `string`, found `42`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:29:6]
//  29 │     g = 123;
//     ·         ─┬─
//     ·          ╰── expected `bool`, found `123`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:33:6]
//  33 │     h = false;
//     ·         ──┬──
//     ·           ╰── expected `int`, found `false`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:37:6]
//  37 │     i = "pi";
//     ·         ──┬─
//     ·           ╰── expected `float`, found `"pi"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:41:6]
//  41 │     j = "true";
//     ·         ───┬──
//     ·            ╰── expected `bool`, found `"true"`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[invalid_assignment_primitives.baml:45:6]
//  45 │     k = true;
//     ·         ──┬─
//     ·           ╰── expected `string`, found `true`
//     ╰────
