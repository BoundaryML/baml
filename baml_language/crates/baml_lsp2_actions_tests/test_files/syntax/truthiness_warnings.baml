// E0167 (B-1563): a non-literal condition whose static type decides the
// branch warns; written literals stay idiomatic. The generic cases pin
// the DEFERRED path: the operand type is an inference variable at check
// time and only resolves at the fixpoint.

class TwCls { x int }

function tw_id<T>(v: T) -> T { v }

function tw_helper() -> int { 1 }

function tw_eager(c: TwCls) -> int {
	if (tw_helper) { 1 } else { 2 };
	if (c) { 3 } else { 4 };
	if (!c) { 5 } else { 6 };
	while (true) { break; }
	if (true) { 7 } else { 8 };
	0
}

function tw_deferred(c: TwCls) -> int {
	if (tw_id(c)) { 1 } else { 2 };
	if (!tw_id(c)) { 3 } else { 4 };
	if (tw_id(null)) { 5 } else { 6 };
	0
}

//----
//- diagnostics
// E0167
//
//   ⚠ this condition is always truthy: a value of type `() -> int throws never` can never be falsy
//     ╭─[truthiness_warnings.baml:13:6]
//  13 │     if (tw_helper) { 1 } else { 2 };
//     ·         ─────────
//     ╰────
// E0167
//
//   ⚠ this condition is always truthy: a value of type `TwCls` can never be falsy
//     ╭─[truthiness_warnings.baml:14:6]
//  14 │     if (c) { 3 } else { 4 };
//     ·         ─
//     ╰────
// E0167
//
//   ⚠ this condition is always truthy: a value of type `TwCls` can never be falsy
//     ╭─[truthiness_warnings.baml:15:7]
//  15 │     if (!c) { 5 } else { 6 };
//     ·          ─
//     ╰────
// E0167
//
//   ⚠ this condition is always truthy: a value of type `TwCls` can never be falsy
//     ╭─[truthiness_warnings.baml:22:6]
//  22 │     if (tw_id(c)) { 1 } else { 2 };
//     ·         ────────
//     ╰────
// E0167
//
//   ⚠ this condition is always truthy: a value of type `TwCls` can never be falsy
//     ╭─[truthiness_warnings.baml:23:7]
//  23 │     if (!tw_id(c)) { 3 } else { 4 };
//     ·          ────────
//     ╰────
// E0167
//
//   ⚠ this condition is always falsy: a value of type `null` can never be truthy
//     ╭─[truthiness_warnings.baml:24:6]
//  24 │     if (tw_id(null)) { 5 } else { 6 };
//     ·         ───────────
//     ╰────
