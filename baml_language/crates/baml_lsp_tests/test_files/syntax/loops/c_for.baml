function SumToTen() -> int {
	let s = 0;

	for (let i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

function InfiniteFor() -> int {
	for (;;) {}

	0
}

// should error everywhere `i` is used.
function NoIteratorDeclared() -> int {
	let s = 0;

	for (; i <= 10; i += 1) {
		s += i;
	}

	s
}

// should be fine
function LoopHeaderScope() -> int {
	let s = 0;

	for (let i = 0; i <= 10; s += i) {
	}

	s
}

// should error in `i += x` (no `x` in scope)
function LoopHeaderScopeUnavailable() -> int {
	let s = 0;

	for (let i = 0; i <= 10; i += x) {
		let x = 1;
	}

	s
}

function BreakContinue() -> int {

	for (;;) {
		break;
		continue;
	}

	0
}

// error: Unknown variable i
//   -->  loops/c_for.baml:21
//    | 
// 20 | 
// 21 | for (; i <= 10; i += 1) {
//    | 
// error: Unknown variable i
//   -->  loops/c_for.baml:22
//    | 
// 21 | for (; i <= 10; i += 1) {
// 22 | s += i;
//    | 
// error: Unknown variable x
//   -->  loops/c_for.baml:42
//    | 
// 41 | 
// 42 | for (let i = 0; i <= 10; i += x) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:21
//    | 
// 20 | 
// 21 | for (; i <= 10; i += 1) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:21
//    | 
// 20 | 
// 21 | for (; i <= 10; i += 1) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:22
//    | 
// 21 | for (; i <= 10; i += 1) {
// 22 | s += i;
//    | 
// error: Error validating: Unknown variable x
//   -->  loops/c_for.baml:42
//    | 
// 41 | 
// 42 | for (let i = 0; i <= 10; i += x) {
//    |

//----
//- diagnostics
// Error: Statement must end with a semicolon.
//     ╭─[ loops_c_for.baml:21:2 ]
//     │
//  21 │ ╭─▶     for (; i <= 10; i += 1) {
//     ┆ ┆   
//  23 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Unknown variable `i`
//     ╭─[ loops_c_for.baml:21:9 ]
//     │
//  21 │     for (; i <= 10; i += 1) {
//     │            ┬  
//     │            ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ loops_c_for.baml:22:8 ]
//     │
//  22 │        s += i;
//     │             ┬  
//     │             ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Unknown variable `i`
//     ╭─[ loops_c_for.baml:21:18 ]
//     │
//  21 │     for (; i <= 10; i += 1) {
//     │                     ┬  
//     │                     ╰── Unknown variable `i`
//     │ 
//     │ Note: Error code: E0003
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_c_for.baml:51:2 ]
//     │
//  51 │ ╭─▶     for (;;) {
//     ┆ ┆   
//  54 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_c_for.baml:32:2 ]
//     │
//  32 │ ╭─▶     for (let i = 0; i <= 10; s += i) {
//  33 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_c_for.baml:12:2 ]
//     │
//  12 │     for (;;) {}
//     │     ─────┬─────  
//     │          ╰─────── Statement must end with a semicolon.
//     │ 
//     │ Note: Error code: E0028
// ────╯
// Error: Statement must end with a semicolon.
//    ╭─[ loops_c_for.baml:4:2 ]
//    │
//  4 │ ╭─▶     for (let i = 1; i <= 10; i += 1) {
//    ┆ ┆   
//  6 │ ├─▶     }
//    │ │           
//    │ ╰─────────── Statement must end with a semicolon.
//    │     
//    │     Note: Error code: E0028
// ───╯
// Error: Statement must end with a semicolon.
//     ╭─[ loops_c_for.baml:42:2 ]
//     │
//  42 │ ╭─▶     for (let i = 0; i <= 10; i += x) {
//     ┆ ┆   
//  44 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Statement must end with a semicolon.
//     │     
//     │     Note: Error code: E0028
// ────╯
