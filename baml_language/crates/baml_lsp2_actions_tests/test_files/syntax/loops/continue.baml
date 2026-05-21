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
// warning: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ loops_continue.baml:23:40 ]
//     │
//  23 │ ╭─▶ function ContinueOutsideLoop() -> int {
//     ┆ ┆   
//  25 │ ├─▶     continue;
//     │ │                   
//     │ ╰─────────────────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
// error: missing return: expected `int`
//     ╭─[ loops_continue.baml:30:33 ]
//     │
//  30 │ ╭─▶ function ContinueNested() -> int {
//     ┆ ┆   
//  36 │ ├─▶ }
//     │ │       
//     │ ╰─────── missing return: expected `int`
//     │     
//     │     Note: Error code: E0029
// ────╯
