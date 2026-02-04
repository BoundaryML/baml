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
// Error: Missing return expression. Function expects `int` but body has no final expression.
//     ╭─[ loops_continue.baml:30:33 ]
//     │
//  30 │ ╭─▶ function ContinueNested() -> int {
//     ┆ ┆   
//  36 │ ├─▶ }
//     │ │       
//     │ ╰─────── Missing return expression. Function expects `int` but body has no final expression.
//     │     
//     │     Note: Error code: E0029
// ────╯
