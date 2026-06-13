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
// error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_positive.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬
//    │                      ╰── Expected ')', found ';'
//    │
//    │ Note: Error code: E0010
// ───╯
// error: Expected block after for expression, found ';'
//    ╭─[ loops_header_requires_let_positive.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬
//    │                      ╰── Expected block after for expression, found ';'
//    │
//    │ Note: Error code: E0010
// ───╯
// error: Expected expression, found ')'
//    ╭─[ loops_header_requires_let_positive.baml:4:27 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                              ┬
//    │                              ╰── Expected expression, found ')'
//    │
//    │ Note: Error code: E0010
// ───╯
// error: assignment is not allowed in expression position; assignment is a statement
//    ╭─[ loops_header_requires_let_positive.baml:4:7 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │          ──┬──
//    │            ╰──── assignment not allowed here
//    │
//    │ Note: Error code: E0010
// ───╯
