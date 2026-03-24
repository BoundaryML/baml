function Factorial(limit: int) -> int {

	let result = 1;

    let res = [1, 2, 3];
    res.push("hi");


	while (true) {
		if (limit == 0) {
			break;
		}
		result = result * limit;
		limit = limit - 1;
	}

	result
}

function OutsideLoop() -> int {

    break;

    3
}

function Nested() -> int {
    let a = 5;
    while (true) {
        while (true) {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
    a // should be 2
}

// error: Error validating: 'break' cannot be used outside of a loop
//   -->  loops/break.baml:19
//    |
// 18 |
// 19 |     break;
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
// Error: type mismatch: expected 1 | 2 | 3, got string
//    ╭─[ loops_break.baml:6:14 ]
//    │
//  6 │     res.push("hi");
//    │              ──┬─  
//    │                ╰─── type mismatch: expected 1 | 2 | 3, got string
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ loops_break.baml:20:32 ]
//     │
//  20 │ ╭─▶ function OutsideLoop() -> int {
//     ┆ ┆   
//  22 │ ├─▶     break;
//     │ │                
//     │ ╰──────────────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
