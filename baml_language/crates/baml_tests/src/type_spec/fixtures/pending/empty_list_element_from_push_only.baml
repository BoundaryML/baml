// pending: S11 method calls contribute bounds
//
// Push-driven element inference in isolation: the function returns void, so
// nothing back-propagates into `?e` - only `xs.push(1)`'s argument can
// resolve it. Requires method resolution (S11) to type the `push` call and
// bound `?e` from its argument.
function f() -> void {
    let xs = [];
    xs.push(1);
    let ys = xs;
     // ^^ int[]
    ys;
}
