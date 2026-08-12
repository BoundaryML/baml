function OuterDeclared() -> int {
	let i = 100;

	for (i = 0; i < 3; i += 1) {
	}

	0
}

function InitWithLet() -> int {
	let s = 0;

	for (let j = 0; j < 3; j += 1) {
		s += j;
	}

	s
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[loops_header_requires_let_positive.baml:4:19]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·                      ┬
//    ·                      ╰── expected `')'`, found `';'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[loops_header_requires_let_positive.baml:4:27]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·                              ┬
//    ·                              ╰── expected `expression`, found `')'`
//    ╰────
// E0010
//
//   × assignment is not allowed in expression position; assignment is a statement
//    ╭─[loops_header_requires_let_positive.baml:4:7]
//  4 │     for (i = 0; i < 3; i += 1) {
//    ·          ──┬──
//    ·            ╰── assignment not allowed here
//    ╰────
