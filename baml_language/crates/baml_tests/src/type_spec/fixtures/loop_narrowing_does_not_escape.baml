//
// B-735: https://linear.app/boundaryml2/issue/B-735
// A binding assigned only inside a loop body must NOT stay narrowed
// after the loop - zero iterations leave the entry value. The post-loop
// environment is loop ENTRY plus the condition's false facts (the loop
// havoc + entry-join rule), so `best` is still `int | null` and the
// `?? 0` is required. TIR leaks the body narrowing and types `best` as
// int after the loop.
function f(xs: int[], limit: int) -> int {
    let best: int? = null;
    for let x in xs {
        best = x;
    }
    best ?? 0
 // ^^^^^^^^^ int
}
