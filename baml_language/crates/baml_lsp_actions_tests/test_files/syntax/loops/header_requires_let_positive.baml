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
