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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unresolved name: i
//    ╭─[ loops_header_requires_let_negative.baml:4:7 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │          ┬  
//    │          ╰── unresolved name: i
//    │ 
//    │ Note: Error code: E0001
// ───╯
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
// Error: unresolved name: i
//    ╭─[ loops_header_requires_let_negative.baml:4:21 ]
//    │
//  4 │     for (i = 0; i < 3; i += 1) {
//    │                        ┬  
//    │                        ╰── unresolved name: i
//    │ 
//    │ Note: Error code: E0001
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
// Error: unresolved name: i
//    ╭─[ loops_header_requires_let_negative.baml:5:8 ]
//    │
//  5 │        s += i;
//    │             ┬  
//    │             ╰── unresolved name: i
//    │ 
//    │ Note: Error code: E0001
// ───╯
