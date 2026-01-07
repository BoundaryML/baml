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
// Error: Expected ')', found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected ')', found ';'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after for expression, found ';'
//    ╭─[ loops_header_requires_let_negative.baml:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected block after for expression, found ';'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found ')'
//    ╭─[ loops_header_requires_let_negative.baml:4:27 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                              ┬  
//    │                              ╰── Expected expression, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_header_requires_let_negative.baml:4:21 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                        ───┬──  
//    │                           ╰──── Statement must end with a semicolon.
//    │ 
//    │ Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_header_requires_let_negative.baml:4:29 ]
//    │
//  4 │ ╭─▶     for (i = 0; i < 3; i += 1) {
//    ┆ ┆   
//  6 │ ├─▶     }
//    │ │           
//    │ ╰─────────── Statement must end with a semicolon.
//    │     
//    │     Note: Error code: E0028
// ───╯
// Error: Unknown variable `i`
//    ╭─[ loops_header_requires_let_negative.baml:4:14 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                 ┬  
//    │                 ╰── Unknown variable `i`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `i`
//    ╭─[ loops_header_requires_let_negative.baml:4:21 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                        ┬  
//    │                        ╰── Unknown variable `i`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable `i`
//    ╭─[ loops_header_requires_let_negative.baml:5:8 ]
//    │
//  5 │        s += i;
//    │             ┬  
//    │             ╰── Unknown variable `i`
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_header_requires_let_negative.baml:14:2 ]
//     │
//  14 │ ╭─▶     for (; s < 2; i += 1) {
//  15 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Unknown variable `i`
//     ╭─[ loops_header_requires_let_negative.baml:14:16 ]
//     │
//  14 │     for (; s < 2; i += 1) {
//     │                   ┬  
//     │                   ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
