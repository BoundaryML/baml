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
// E0146
//
//   ⚠ unreachable code: 1 statement(s) after diverging statement
//     ╭─[loops_continue.baml:25:5]
//  25 │     continue;
//     ·     ─────────
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[loops_continue.baml:30:34]
//  30 │ ╭─▶ function ContinueNested() -> int {
//  31 │ │       while (true) {
//  32 │ │           while (true) {
//  33 │ │               continue;
//  34 │ │           }
//  35 │ │       }
//  36 │ ├─▶ }
//     · ╰──── expected `int`, found `void`
//     ╰────
