// Native backend fixture: closures capturing locals, parameters and other
// closures; mutation through a capture seen both ways; per-iteration cells.

class Box { n: int }

function counter(n: int) -> int {
    let count = 0;
    let bump = () -> void { count = count + 1; };
    let i = 0;
    while (i < n) { bump(); i = i + 1; }
    count
}

function seen_both_ways(a: int) -> int {
    let x = a;
    let read = () -> int { x };
    x = x + 1;
    let after_outer_write = read();
    let write = (v: int) -> void { x = v; };
    write(100);
    after_outer_write * 1000 + x
}

function make_adder(k: int) -> (int) -> int throws never {
    (x: int) -> int { x + k }
}

function escaped(a: int) -> int {
    let add = make_adder(a);
    add(1) + add(2)
}

function make_counter() -> () -> int throws never {
    let count = 0;
    () -> int { count = count + 1; count }
}

function two_counters(n: int) -> int {
    let first = make_counter();
    let second = make_counter();
    let i = 0;
    while (i < n) { first(); i = i + 1; }
    first() * 100 + second()
}

function captured_param(a: int) -> int {
    let double = () -> void { a = a * 2; };
    double();
    double();
    a
}

function loop_capture(n: int) -> int {
    let fs: (() -> int throws never)[] = [];
    for (let x in [1, 2, 3]) {
        fs.push(() -> int { x * n });
    }
    let total = 0;
    for (let f in fs) { total = total + f(); }
    total
}

function cfor_capture(n: int) -> int {
    let fs: (() -> int throws never)[] = [];
    for (let i = 0; i < n; i = i + 1) {
        fs.push(() -> int { i });
    }
    let total = 0;
    for (let f in fs) { total = total + f(); }
    total
}

function while_capture(n: int) -> int {
    let fs: (() -> int throws never)[] = [];
    let i = 0;
    while (i < n) {
        let j = i;
        fs.push(() -> int { j });
        i = i + 1;
    }
    let total = 0;
    for (let f in fs) { total = total + f(); }
    total
}

function shared_loop_var(n: int) -> int {
    let fs: (() -> int throws never)[] = [];
    let i = 0;
    while (i < n) {
        fs.push(() -> int { i });
        i = i + 1;
    }
    let total = 0;
    for (let f in fs) { total = total + f(); }
    total
}

function nested(a: int) -> int {
    let k = a;
    let outer = (x: int) -> int {
        let inner = (y: int) -> int { x + y + k };
        inner(1)
    };
    outer(2)
}

function closure_captures_closure(a: int) -> int {
    let base = (x: int) -> int { x + a };
    let twice = (x: int) -> int { base(base(x)) };
    twice(1)
}

function mutate_box(b: Box) -> int {
    let set = (v: int) -> void { b.n = v; };
    set(5);
    b.n
}

function box_field(a: int) -> int {
    let b = Box { n: a };
    let bump = () -> void { b.n = b.n + 1; };
    bump();
    bump();
    b.n
}

function captured_array(a: int) -> int {
    let xs = [a];
    let push = (v: int) -> void { xs.push(v); };
    push(a + 1);
    push(a + 2);
    xs.length() * 1000 + xs[2]
}

function rebound(a: int) -> int {
    let xs = [a];
    let swap = () -> void { xs = [a + 1, a + 2]; };
    let first = xs[0];
    swap();
    first * 1000 + xs[1]
}

function throwing_capture(a: int) -> int {
    let d = a;
    let div = (x: int) -> int throws baml.panics.DivisionByZero { x / d };
    div(100) catch (e) { baml.panics.DivisionByZero => -1 }
}

function with_default(a: int, step: int = 10) -> int {
    let bump = () -> int { step = step + 1; step };
    bump() + a
}

function strings(s: string) -> string {
    let acc = s;
    let add = (x: string) -> void { acc = acc + x; };
    add("-");
    add(s);
    acc
}
