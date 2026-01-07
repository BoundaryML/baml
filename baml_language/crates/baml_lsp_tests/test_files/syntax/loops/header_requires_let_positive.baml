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
// Error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_positive.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ';'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after for expression, found ';'
//    ╭─[ loops_header_requires_let_positive.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected block after for expression, found ';'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ loops_header_requires_let_positive.baml:4:27 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                              ┬  
//    │                              ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_header_requires_let_positive.baml:4:21 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                        ───┬──  
//    │                           ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_header_requires_let_positive.baml:4:29 ]
//    │
//  4 │ ╭─▶     for (i = 0; i < 3; i += 1) {
//  5 │ ├─▶     }
//    │ │           
//    │ ╰─────────── Statement must end with a semicolon.
//    │     
//    │     Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_header_requires_let_positive.baml:13:2 ]
//     │
//  13 │ ╭─▶     for (let j = 0; j < 3; j += 1) {
//     ┆ ┆   
//  15 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
