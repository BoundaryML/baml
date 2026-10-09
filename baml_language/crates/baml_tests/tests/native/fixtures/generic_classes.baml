// Generic classes: one generated struct per type-argument tuple.

class Box<T> {
    value: T,
    function get(self) -> T { self.value }
    function set(self, v: T) -> void { self.value = v; }
    function swap(self, other: Box<T>) -> void {
        let mine = self.value;
        self.value = other.value;
        other.value = mine;
    }
    function map<U>(self, f: (T) -> U throws never) -> Box<U> { Box<U> { value: f(self.value) } }
}

class Pair<A, B> {
    first: A,
    second: B,
    function flip(self) -> Pair<B, A> { Pair<B, A> { first: self.second, second: self.first } }
}

class Stack<T> {
    items: T[],
    function push(self, x: T) -> void { self.items.push(x); }
    function peek(self) -> T | null {
        if (self.items.length() == 0) { return null; }
        self.items[self.items.length() - 1]
    }
    function size(self) -> int { self.items.length() }
}

class Pt { x: int, y: int }

function make_box<T>(x: T) -> Box<T> { Box<T> { value: x } }

function unbox<T>(b: Box<T>) -> T { b.get() }

function boxes(n: int) -> int {
    let b = Box<int> { value: n };
    let c = Box { value: n + 1 };
    let d = make_box(n + 2);
    b.set(b.get() * 2);
    b.swap(c);
    let e = b.map((x: int) -> string { "<" + x.to_string() + ">" });
    let f = d.map((x: int) -> Box<int> { Box<int> { value: x } });
    unbox(b) + c.get() + unbox(d) + e.value.length() + f.get().get()
}

function pairs(n: int, s: string) -> string {
    let p = Pair<int, string> { first: n, second: s };
    let q = p.flip();
    let r = Pair { first: Pt { x: n, y: 1 }, second: [s] };
    q.first + p.first.to_string() + r.first.x.to_string() + r.second[0]
}

function stacks(n: int) -> int {
    let s = Stack<int> { items: [] };
    let i = 0;
    while (i < n) { s.push(i * i); i = i + 1; }
    let top = s.peek() ?? -1;
    let names = Stack<string> { items: ["a"] };
    names.push("b");
    let last = names.peek() ?? "";
    top + s.size() + last.length()
}

function nested(n: int) -> int {
    let bb = Box<Box<int>> { value: Box<int> { value: n } };
    let outer = make_box(bb);
    let inner = unbox(unbox(unbox(outer)));
    bb.value.set(inner + 1);
    bb.get().get()
}

function renders(n: int) -> string {
    let b = Box<int> { value: n };
    let p = Pair<string, Box<int>> { first: "s", second: b };
    let j = baml.json.to_string(p);
    let back = baml.json.deserialize<Pair<string, Box<int>>>(j);
    b.to_string() + "|" + p.to_string() + "|" + j + "|" + back.second.get().to_string()
}

function type_tests(n: int) -> int {
    let b = Box<int> { value: n };
    let u: Box<int> | Box<string> | int = b;
    let v: Box<int> | Box<string> | int = Box<string> { value: "s" };
    let w: Box<int> | Box<string> | int = n;
    let a = if (u is Box<int>) { 1 } else { 0 };
    let c = if (v is Box<int>) { 10 } else { 0 };
    let d = if (v is Box<string>) { 100 } else { 0 };
    let e = match w { let k: int => k, let x: Box<int> => x.value, _ => 0 };
    let f = match u { let x: Box<int> => x.get(), let y: Box<string> => y.value.length(), _ => 0 };
    a + c + d + e + f
}

function nullable_boxes(n: int) -> int {
    let b: Box<int> | null = if (n > 0) { Box<int> { value: n } } else { null };
    if (b != null) { b.get() } else { -1 }
}
