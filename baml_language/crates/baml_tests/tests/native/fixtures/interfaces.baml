// Interfaces: an interface-typed value is an enum over the program's
// implementors; a call on a concrete receiver is static, on an interface
// value a match.

interface Shape {
    function area(self) -> int throws never
    function name(self) -> string throws never
    function describe(self) -> string throws never { self.name() + "=" + self.area().to_string() }
}

class Square { side: int }
class Rect { w: int, h: int }
class Dot { }

implement Shape for Square {
    function area(self) -> int { self.side * self.side }
    function name(self) -> string { "square" }
}

implement Shape for Rect {
    function area(self) -> int { self.w * self.h }
    function name(self) -> string { "rect" }
    function describe(self) -> string { "rect " + self.w.to_string() + "x" + self.h.to_string() }
}

implement Shape for Dot {
    function area(self) -> int { 0 }
    function name(self) -> string { "dot" }
}

interface Named {
    name: string,
    function greet(self) -> string throws never
    function shout(self) -> string throws never { self.greet() + "!" }
    function rename(self, to: string) -> void throws never { self.name = to; }
}

class Dog {
    name: string,
    barks: int,
    implements Named {
        function greet(self) -> string { "woof " + self.name }
    }
}
class Cat {
    name: string,
    implements Named {
        function greet(self) -> string { "meow " + self.name }
        function shout(self) -> string { "MEOW" }
    }
}

interface Counted {
    function count(self) -> int throws never
}
implement Counted for int {
    function count(self) -> int { self }
}
implement Counted for string {
    function count(self) -> int { self.length() }
}
implement Counted for Dog {
    function count(self) -> int { self.barks }
}

function static_calls(n: int) -> string {
    let sq = Square { side: n };
    let r = Rect { w: n, h: 2 };
    let d = Dot { };
    sq.describe() + "|" + r.describe() + "|" + d.describe() + "|" + sq.name() + sq.area().to_string()
}

function total(shapes: Shape[]) -> int {
    let t = 0;
    for (let s in shapes) { t = t + s.area(); }
    t
}

function describe_all(shapes: Shape[]) -> string {
    let out = "";
    for (let s in shapes) { out = out + s.describe() + ";"; }
    out
}

function shapes(n: int) -> string {
    let all: Shape[] = [Square { side: n }, Rect { w: n, h: 3 }, Dot { }];
    let one: Shape = Rect { w: 2, h: n };
    total(all).to_string() + "/" + describe_all(all) + "/" + one.describe() + "/" + one.area().to_string()
}

function narrowing(n: int) -> int {
    let s: Shape = if (n > 0) { Square { side: n } } else { Rect { w: 1, h: n } };
    let a = if (s is Square) { 1 } else { 0 };
    let b = match s { let q: Square => q.side, let r: Rect => r.w * 100, _ => -1 };
    let c = match s { Square => 10, Rect => 20, Dot => 30, _ => 40 };
    a + b + c
}

function switch_arms(n: int) -> int {
    let s: Shape = if (n > 2) { Square { side: n } } else if (n > 0) { Rect { w: n, h: n } } else { Dot { } };
    match s {
        let q: Square => q.side,
        let r: Rect => r.w + r.h,
        let d: Dot => -1,
        _ => -2,
    }
}

function bounded<T extends Shape>(x: T) -> string { x.describe() + ":" + x.area().to_string() }

function bounded_calls(n: int) -> string {
    bounded(Square { side: n }) + "," + bounded(Rect { w: n, h: 1 }) + "," + bounded(Dot { })
}

function fields(n: int) -> string {
    let d = Dog { name: "rex", barks: n };
    let c = Cat { name: "tom" };
    let pets: Named[] = [d, c];
    let out = "";
    for (let p in pets) {
        out = out + p.name + ":" + p.greet() + ":" + p.shout() + ";";
        p.rename(p.name + "y");
    }
    out + d.name + c.name + d.shout() + c.shout()
}

function primitives(n: int, s: string) -> int {
    let items: Counted[] = [n, s, Dog { name: "d", barks: n + 1 }];
    let total = 0;
    for (let item in items) { total = total + item.count(); }
    total + n.count() + s.count()
}

function nullable(n: int) -> string {
    let s: Shape | null = if (n > 0) { Square { side: n } } else { null };
    if (s == null) { return "none"; }
    s.describe()
}

function renders(n: int) -> string {
    let s: Shape = Square { side: n };
    let pets: Named[] = [Dog { name: "a", barks: n }, Cat { name: "b" }];
    s.to_string() + "|" + baml.json.to_string(s) + "|" + baml.json.to_string(pets) + "|" + pets.to_string()
}

function in_union(n: int) -> string {
    let u: Shape | int = if (n > 0) { Square { side: n } } else { n };
    let v: Shape | int = Rect { w: 1, h: 1 };
    let a = match u { let s: Shape => s.describe(), let i: int => "int" + i.to_string() };
    let b = if (v is Shape) { "shape" } else { "int" };
    a + b
}

class Holder { item: Shape, tag: string }

function in_field(n: int) -> string {
    let h = Holder { item: Square { side: n }, tag: "t" };
    let first = h.item.describe();
    h.item = Dot { };
    first + h.item.name() + h.to_string()
}

class Money {
    cents: int,
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never { self.cents.cmp(other.cents) }
    }
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.cents == other.cents }
    }
}

function comparisons(a: int, b: int) -> string {
    let m = Money { cents: a };
    let n = Money { cents: b };
    let x = a.to_string();
    let y = b.to_string();
    let fa: float = if (a > b) { 1.5 } else { 0.5 };
    let bits = [
        a.lt(b), a.le(b), a.gt(b), a.ge(b),
        (a > 0).lt(b > 0), (a > 0).ge(b > 0),
        x.lt(y), x.ge(y),
        fa.lt(1.0), fa.gt(1.0),
        m.lt(n), m.le(n), m.gt(n), m.ge(n), m.eq(n), m.cmp(n) == baml.ops.Ordering.Less,
    ];
    let out = "";
    for (let bit in bits) { out = out + (if (bit) { "1" } else { "0" }); }
    out
}

// A union of implementors as a receiver dispatches like an interface value;
// an `implements` block may link an interface field to a field of another
// name and declare defaulted parameters past the interface's.

interface Labeled {
    label: string,
    function show(self, prefix: string) -> string throws never
}

class Tag {
    tag_text: string,
    implements Labeled {
        label as tag_text
        function show(self, prefix: string, suffix: string = "!") -> string { prefix + self.tag_text + suffix }
    }
}

class Note {
    label: string,
    implements Labeled {
        function show(self, prefix: string) -> string { prefix + self.label }
    }
}

function union_receivers(n: int) -> string {
    let u: Tag | Note = if (n > 0) { Tag { tag_text: "t" + n.to_string() } } else { Note { label: "n" } };
    let v: Tag | Note | null = if (n > 1) { Note { label: "v" } } else { null };
    let w: Labeled = Tag { tag_text: "w" };
    let a = u.show("<");
    let b = if (v != null) { v.show("[") } else { "none" };
    let c = w.label + w.show("(");
    w.label = "ww";
    let d = match u { let t: Tag => t.tag_text, let m: Note => m.label };
    let e = u.label;
    u.label = e + "+";
    a + "|" + b + "|" + c + "|" + w.label + "|" + d + "|" + e + u.label
}
