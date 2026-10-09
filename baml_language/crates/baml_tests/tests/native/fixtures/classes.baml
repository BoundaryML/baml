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
