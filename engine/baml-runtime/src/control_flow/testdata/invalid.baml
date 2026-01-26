//# This file intentionally contains a syntax error to verify error handling.
function Invalid() -> string {
    //# Previously missing a closing parenthesis
    let x = if (true) {
        "ok"
    } else {
        "bad"
    };

    x
}
