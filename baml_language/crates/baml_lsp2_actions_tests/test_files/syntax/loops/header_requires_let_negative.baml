function MissingLetInit() -> int {
	let s = 0;

	for (i = 0; i < 3; i += 1) {
		s += i;
	}

	s
}

function MissingLetAfter() -> int {
	let s = 0;

	for (; s < 2; i += 1) {
	}

	0
}

// error: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:4
//    |
//  3 |
//  4 | for (i = 0; i < 3; i += 1) {
//    |
// error: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:4
//    |
//  3 |
//  4 | for (i = 0; i < 3; i += 1) {
//    |
// error: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:5
//    |
//  4 | for (i = 0; i < 3; i += 1) {
//  5 | s += i;
//    |
// error: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:14
//    |
// 13 |
// 14 | for (; s < 2; i += 1) {
//    |
// error: Error validating: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:4
//    |
//  3 |
//  4 | for (i = 0; i < 3; i += 1) {
//    |
// error: Error validating: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:4
//    |
//  3 |
//  4 | for (i = 0; i < 3; i += 1) {
//    |
// error: Error validating: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:4
//    |
//  3 |
//  4 | for (i = 0; i < 3; i += 1) {
//    |
// error: Error validating: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:5
//    |
//  4 | for (i = 0; i < 3; i += 1) {
//  5 | s += i;
//    |
// error: Error validating: Unknown variable i
//   -->  loops/header_requires_let_negative.baml:14
//    |
// 13 |
// 14 | for (; s < 2; i += 1) {
//    |

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[loops_header_requires_let_negative.baml:4:19]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·                      ┬
//    ·                      ╰── expected `')'`, found `';'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[loops_header_requires_let_negative.baml:4:27]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·                              ┬
//    ·                              ╰── expected `expression`, found `')'`
//    ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//    ╭─[loops_header_requires_let_negative.baml:4:7]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·          ──┬──
//    ·            ╰── assignment not allowed here
//    ╰────
// E0003
//
//   × unresolved name: `i`
//     ╭─[loops_header_requires_let_negative.baml:14:16]
//  14 │     for (; s < 2; i += 1) {
//     ·                   ─
//     ╰────
