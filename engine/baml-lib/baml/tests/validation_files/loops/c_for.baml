fn SumToTen() -> int {
	let mut s = 0;

	for (let mut i = 1; i <= 10; i += 1) {
		s += i;
	}

	s
}

// should error everywhere `i` is used.
fn NoIteratorDeclared() -> int {
	let mut s = 0;

	for (; i <= 10; i += 1) {
		s += i;
	}

	s
}

// should be fine
fn LoopHeaderScope() -> int {
	let mut s = 0;

	for (let mut i = 0; i <= 10; s += i) {
	}

	s
}

// should error in `i += x` (no `x` in scope)
fn LoopHeaderScopeUnavailable() -> int {
	let mut s = 0;

	for (let mut i = 0; i <= 10; i += x) {
		let x = 1;
	}

	s
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/c_for.baml:1
//    | 
//    | 
//  1 | fn SumToTen() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/c_for.baml:12
//    | 
// 11 | // should error everywhere `i` is used.
// 12 | fn NoIteratorDeclared() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/c_for.baml:23
//    | 
// 22 | // should be fine
// 23 | fn LoopHeaderScope() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/c_for.baml:33
//    | 
// 32 | // should error in `i += x` (no `x` in scope)
// 33 | fn LoopHeaderScopeUnavailable() -> int {
//    | 
// error: Unknown variable i
//   -->  loops/c_for.baml:16
//    | 
// 15 | for (; i <= 10; i += 1) {
// 16 | s += i;
//    | 
// error: Unknown variable x
//   -->  loops/c_for.baml:36
//    | 
// 35 | 
// 36 | for (let mut i = 0; i <= 10; i += x) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:15
//    | 
// 14 | 
// 15 | for (; i <= 10; i += 1) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:15
//    | 
// 14 | 
// 15 | for (; i <= 10; i += 1) {
//    | 
// error: Error validating: Unknown variable i
//   -->  loops/c_for.baml:16
//    | 
// 15 | for (; i <= 10; i += 1) {
// 16 | s += i;
//    | 
// error: Error validating: Unknown variable x
//   -->  loops/c_for.baml:36
//    | 
// 35 | 
// 36 | for (let mut i = 0; i <= 10; i += x) {
//    | 
