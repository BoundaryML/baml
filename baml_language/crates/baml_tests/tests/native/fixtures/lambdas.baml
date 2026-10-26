// Native backend fixture: lambdas without captures, function values and
// calls through them.

function plain(x: int) -> int { x + 1 }

function twice(f: (int) -> int throws never, a: int) -> int { f(f(a)) }

function direct(a: int) -> int {
    let double = (x: int) -> int { x * 2 };
    double(a) + plain(a)
}

function named_value(a: int) -> int {
    let g = plain;
    g(a) + twice(plain, a)
}

function passed(a: int) -> int {
    twice((x: int) -> int { x - 1 }, a)
}

function returned(which: int) -> (int) -> int throws never {
    if (which == 0) { plain } else { (x: int) -> int { x * x } }
}

function chosen(which: int, a: int) -> int {
    let f = returned(which);
    f(a)
}

function void_lambda(a: int) -> int {
    let noop = (x: int) -> void { };
    noop(a);
    a
}

function throwing(a: int) -> int {
    let check = (x: int) -> int throws baml.panics.DivisionByZero { 100 / x };
    check(a)
}

function caught(a: int) -> int {
    let check = (x: int) -> int throws baml.panics.DivisionByZero { 100 / x };
    check(a) catch (e) { baml.panics.DivisionByZero => -1 }
}

function two_args(a: int, b: int) -> int {
    let sub = (x: int, y: int) -> int { x - y };
    sub(a, b) + sub(b, a)
}

function strings(s: string) -> string {
    let shout = (x: string) -> string { x + "!" };
    shout(shout(s))
}

function in_array(a: int) -> int {
    let fs: ((int) -> int throws never)[] = [plain, (x: int) -> int { x * 3 }];
    let total = 0;
    for (let f in fs) { total = total + f(a); }
    total
}

function nullable_fn(which: int, a: int) -> int {
    let f: ((int) -> int throws never) | null = null;
    if (which != 0) { f = plain; }
    match (f) {
        null => -1,
        let g: (int) -> int throws never => g(a),
    }
}

function recursion_through_value(n: int) -> int {
    let step = recursion_through_value;
    if (n <= 0) { 0 } else { step(n - 1) + 1 }
}
