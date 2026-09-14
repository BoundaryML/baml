// S12: `catch` is narrowing on the error channel - the base's effect
// collects into its own channel, arms subtract what they handle, and
// the handled-everything case leaves the enclosing function throwing
// never.
function risky(c: bool) -> int {
    if (c) {
        throw "bad";
    }
    1
}

function g(c: bool) -> int {
    let r = risky(c) catch_all (e) { _ => 0 };
     // ^ int
    r
}
