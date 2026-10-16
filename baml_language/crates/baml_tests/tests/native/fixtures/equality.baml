// Native backend fixture: the broad `==` on objects, as the VM's EqualsDriver
// decides it: an object equals itself, arrays element by element, maps entry
// by entry, class instances field by field or by the class's own
// `baml.ops.Equals`, enums by variant or by their own `eq`, a cycle met again
// taken as equal, and a nested `eq` at the frame limit.

class Point { x: int, y: float }
class Line { a: Point, b: Point, tags: string[], meta: map<string, int> }
class Node { n: int, next: Node | null }

class Cents {
    amount: int,
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.amount / 100 == other.amount / 100 }
    }
}
class Wallet { coins: Cents[], label: string }

enum Suit { Hearts, Spades }
enum Loose { A, B }
implements baml.ops.Equals for Loose {
    function eq(self, other: Loose) -> bool throws never { true }
}

function same_object(n: int) -> bool {
    let p = Point { x: n, y: 1.5 };
    let q = p;
    p == q
}

function structural(n: int) -> bool {
    let p = Point { x: n, y: 0.0 / 0.0 };
    let q = Point { x: n, y: 0.0 / 0.0 };
    p == q
}

function differing(n: int) -> bool {
    Point { x: n, y: 1.0 } == Point { x: n + 1, y: 1.0 }
}

function nested(n: int) -> bool {
    let l = Line { a: Point { x: n, y: 1.0 }, b: Point { x: 2, y: 2.0 }, tags: ["a", "b"], meta: { "k": n } };
    let m = Line { a: Point { x: n, y: 1.0 }, b: Point { x: 2, y: 2.0 }, tags: ["a", "b"], meta: { "k": n } };
    let o = Line { a: Point { x: n, y: 1.0 }, b: Point { x: 2, y: 2.0 }, tags: ["a"], meta: { "k": n } };
    let q = Line { a: Point { x: n, y: 1.0 }, b: Point { x: 2, y: 2.0 }, tags: ["a", "b"], meta: { "k": n + 1 } };
    l == m && !(l == o) && !(l == q)
}

function arrays_and_maps(n: int) -> bool {
    let xs = [1, 2, n];
    let ys = [1, 2, n];
    let m1 = { "a": 1, "b": n };
    let m2 = { "b": n, "a": 1 };
    let m3 = { "a": 1 };
    xs == ys && !(xs == [1, 2]) && m1 == m2 && !(m1 == m3)
}

function cyclic(n: int) -> bool {
    let a = Node { n: n, next: null };
    let b = Node { n: n, next: null };
    a.next = a;
    b.next = b;
    let c = Node { n: n + 1, next: null };
    c.next = c;
    a == b && !(a == c)
}

function overridden(n: int) -> bool {
    let w1 = Wallet { coins: [Cents { amount: 100 + n }, Cents { amount: 250 }], label: "w" };
    let w2 = Wallet { coins: [Cents { amount: 199 }, Cents { amount: 200 }], label: "w" };
    let w3 = Wallet { coins: [Cents { amount: 100 + n }, Cents { amount: 300 }], label: "w" };
    let direct = Cents { amount: 150 } == Cents { amount: 199 };
    (w1 == w2) && !(w1 == w3) && direct
}

function enums(n: int) -> bool {
    let s = if (n > 0) { Suit.Hearts } else { Suit.Spades };
    (s == Suit.Hearts) == (n > 0) && Loose.A == Loose.B
}

function unions(n: int) -> bool {
    let u: Point | int = if (n > 0) { Point { x: n, y: 1.0 } } else { n };
    let v: Point | int = Point { x: n, y: 1.0 };
    u == v
}

function deep_eq(n: int) -> bool {
    if (n == 0) { Cents { amount: 1 } == Cents { amount: 2 } } else { deep_eq(n - 1) }
}
