// Push-driven element inference in isolation (S11): the function returns
// void, so nothing back-propagates into the empty list's element var -
// only `xs.push(1)`'s argument bound resolves it.
function f() -> void {
    let xs = [];
    xs.push(1);
    let ys = xs;
     // ^^ int[]
    ys;
}
