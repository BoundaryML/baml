// Generic functions: one native instance per type-argument tuple.

class Box { n: int }
class Pt { x: int, y: int }

function identity<T>(x: T) -> T { x }

function first<T>(xs: T[]) -> T { xs[0] }

function last_or<T>(xs: T[], fallback: T) -> T {
    if (xs.length() == 0) { return fallback; }
    xs[xs.length() - 1]
}

function find_first<T>(xs: T[], wanted: T) -> T | null {
    for (let x in xs) {
        if (x == wanted) { return x; }
    }
    null
}

function wrap<T>(x: T) -> T[] {
    let out: T[] = [];
    out.push(x);
    out
}

function twice<T>(x: T) -> T[] {
    let once = wrap(x);
    once.push(identity(x));
    once
}

function apply<T, U>(x: T, f: (T) -> U throws never) -> U { f(x) }

function count_down<T>(xs: T[], i: int) -> int {
    if (i <= 0) { return xs.length(); }
    count_down(xs, i - 1) + 1
}

function render<T>(x: T) -> string {
    x.to_string() + "/" + baml.json.to_string(x)
}

function is_int_arg<T>(x: T) -> bool { x is int }

function with_lambda<T>(xs: T[], fallback: T) -> T {
    let pick = (ys: T[]) -> T { last_or(ys, fallback) };
    pick(xs)
}

function via_value<T>(x: T) -> T {
    let f = identity<T>;
    f(x)
}

function use_ints(n: int) -> int {
    let a = identity(n);
    let b = identity<int>(n + 1);
    let xs = [n, n + 2, n + 4];
    let c = first(xs);
    let d = last_or(xs, 0);
    let empty: int[] = [];
    let e = last_or(empty, 7);
    let f = find_first(xs, n + 2) ?? -1;
    let g = find_first(xs, 1000) ?? -1;
    let h = twice(n);
    let i = count_down(xs, 3);
    let j = apply(n, (v: int) -> int { v * 2 });
    let k = with_lambda(xs, 9);
    let l = via_value(n);
    a + b + c + d + e + f + g + h[0] + h[1] + i + j + k + l
}

function use_strings(s: string) -> string {
    let a = identity(s);
    let b = first([s, "b"]);
    let none: string[] = [];
    let c = last_or(none, "fallback");
    let d = find_first([s, "z"], "z") ?? "none";
    let e = apply(s, (v: string) -> int { v.length() });
    let f = twice(s);
    a + b + c + d + e.to_string() + f[1] + with_lambda([s], "w") + via_value(s)
}

function use_floats(x: float) -> float {
    identity(x) + first([x, 2.0]) + last_or([], 0.5) + (find_first([x], x) ?? 0.0)
}

function use_classes(n: int) -> int {
    let b = identity(Box { n: n });
    let p = first([Pt { x: n, y: 1 }, Pt { x: 2, y: 2 }]);
    let boxes = twice(Box { n: n + 1 });
    let pick = apply(p, (q: Pt) -> int { q.x + q.y });
    b.n + p.x + boxes[1].n + pick + count_down(boxes, 2)
}

function use_arrays(n: int) -> int {
    let nested = identity([n, n]);
    let rows = first([[n], [n, n]]);
    let more = twice([n]);
    nested[0] + rows.length() + more[0][0] + more.length()
}

function use_nullable(n: int) -> int {
    let a: int | null = identity(null);
    let b = identity<int | null>(n);
    let holes: (int | null)[] = [null, n];
    let c = first(holes);
    let a2 = a ?? 1;
    let b2 = b ?? 2;
    let c2 = if (c == null) { 30 } else { 3 };
    a2 + b2 + c2
}

function renders(n: int) -> string {
    render(n) + render("s") + render(Box { n: n }) + render([n]) + render(true)
}

function type_tests(n: int) -> int {
    let a = is_int_arg(n);
    let b = is_int_arg("x");
    let c = is_int_arg(Box { n: n });
    (if (a) { 1 } else { 0 }) + (if (b) { 10 } else { 0 }) + (if (c) { 100 } else { 0 })
}
