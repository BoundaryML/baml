type X // c
= int;

client K // c
= "x";

class C {
    x // c
    : int,
    y: int // c
    @alias("y"),
}

implement I for C {
    type Y // c
    = int;
    a // c
    as b,
}

function f(a // c
: int, b: int // c
= 1, m: Map<int, Item // c
= int>) -> int {
    let x // c
    = 1;
    x // c
    += 1;
    let o = { a // c
    : 1 };
    match (x) {
        1 // c
        => 2,
        _ => 3,
    }
    let z = foo() // c
    catch (e) { _ => 0 };
    let r = g() catch (e) // d
    { _ => 1, };
    let r2 = g() catch // c
    (e) { _ => 1, };
    let w = x // c
    else { return 0; };
    if let y // c
    = x {
        1;
    }
    let e = (x: int) // c4
    -> int { x };
    let l = spawn task // c12
    with a { 1 };
    let i = x is // c
    int;
    let j = o. // c
    a;
    x
}
