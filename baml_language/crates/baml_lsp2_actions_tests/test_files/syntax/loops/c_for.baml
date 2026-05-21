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
// error: unresolved name: i
//     ╭─[ loops_c_for.baml:21:9 ]
//     │
//  21 │     for (; i <= 10; i += 1) {
//     │            ┬  
//     │            ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: i
//     ╭─[ loops_c_for.baml:22:8 ]
//     │
//  22 │        s += i;
//     │             ┬  
//     │             ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: i
//     ╭─[ loops_c_for.baml:21:18 ]
//     │
//  21 │     for (; i <= 10; i += 1) {
//     │                     ┬  
//     │                     ╰── unresolved name: i
//     │ 
//     │ Note: Error code: E0003
// ────╯
// error: unresolved name: x
//     ╭─[ loops_c_for.baml:42:32 ]
//     │
//  42 │     for (let i = 0; i <= 10; i += x) {
//     │                                   ┬  
//     │                                   ╰── unresolved name: x
//     │ 
//     │ Note: Error code: E0003
// ────╯
// warning: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ loops_c_for.baml:51:12 ]
//     │
//  51 │ ╭─▶     for (;;) {
//  52 │ ├─▶        break;
//     │ │                   
//     │ ╰─────────────────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
