// Native backend fixture: data classes, JSON and rendering.

class Point { x: int, y: int }
class Box { name: string, corner: Point, tag: string | null, weights: int[] }

function area(p: Point) -> int {
    p.x * p.y
}

function make(n: int) -> string {
    let p = Point { x: n, y: n + 1 };
    let b = Box { name: "b" + n.to_string(), corner: p, tag: null, weights: [n, n * 2] };
    b.corner.x = b.corner.x + 1;
    p.y = 0;
    baml.json.to_string(b)
}

function roundtrip(raw: string) -> int {
    let p = baml.json.deserialize<Point>(raw);
    area(p)
}

function box_roundtrip(raw: string) -> string {
    let b = baml.json.deserialize<Box>(raw);
    baml.json.to_string(b)
}

function render(n: int) -> string {
    let p = Point { x: n, y: -n };
    p.to_string()
}

function nullable(n: int) -> string {
    let b = Box { name: "t", corner: Point { x: 0, y: 0 }, tag: null, weights: [] };
    if (n > 0) {
        b.tag = "set";
    }
    if (b.tag == null) {
        "none"
    } else {
        "some"
    }
}

// A field passed directly as an argument while the callee writes to the
// object that owns the field through another handle. The VM sees one
// object; native code must not hold a borrow of `h` across the call.
class Counter { n: int }
class Holder { c: Counter, total: int }

function bump(c: Counter, h: Holder) -> int {
    h.total = h.total + 1;
    c.n = c.n + h.total;
    c.n
}

function borrowed_argument(n: int) -> int {
    let h = Holder { c: Counter { n: n }, total: 0 };
    bump(h.c, h) + bump(h.c, h) + h.total
}
