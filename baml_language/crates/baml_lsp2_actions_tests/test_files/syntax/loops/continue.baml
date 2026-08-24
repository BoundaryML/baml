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

// Neither loop holds a `break`, so both diverge and this tail-less body is
// `never`, not `void` — the declared `int` return is satisfied by never
// returning. (This used to report E0001 `expected int, found void`.)
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
