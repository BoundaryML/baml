function Factorial(limit: int) -> int {

	let result = 1;


    // used to make the loop break without relying on `break` implementation.
    let should_continue = true;
	while (should_continue) {
		result = result * limit;
		limit = limit - 1;

        if (limit != 0) {
            continue;
        } else {
            should_continue = false;
        }

	}

	result
}

function ContinueOutsideLoop() -> int {

    continue;

    3
}

function ContinueNested() -> int {
    while (true) {
        while (true) {
            continue;
        }
    }
}

// error: Error validating: 'continue' cannot be used outside of a loop
//   -->  loops/continue.baml:25
//    | 
// 24 | 
// 25 |     continue;
//    |

//----
//- diagnostics
// Error: Expected `int`, found `void`
//    ╭─[ loops_continue.baml:1:1 ]
//    │
//  1 │ function Factorial(limit: int) -> int {
//    │ │ 
//    │ ╰─ Expected `int`, found `void`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `1`, found `int`
//    ╭─[ loops_continue.baml:9:12 ]
//    │
//  9 │        result = result * limit;
//    │                 ───────┬──────  
//    │                        ╰──────── Expected `1`, found `int`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `true`, found `false`
//     ╭─[ loops_continue.baml:15:31 ]
//     │
//  15 │             should_continue = false;
//     │                               ──┬──  
//     │                                 ╰──── Expected `true`, found `false`
//     │ 
//     │ Note: Error code: E0001
// ────╯
