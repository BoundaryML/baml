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
// Error: Expected RParen, found Semicolon
//    ╭─[ 0:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected RParen, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected block after for expression, found Semicolon
//    ╭─[ 0:4:19 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                      ┬  
//    │                      ╰── Expected block after for expression, found Semicolon
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found RParen
//    ╭─[ 0:4:27 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                              ┬  
//    │                              ╰── Expected expression, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
