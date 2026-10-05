enum KeyKind {
    Foo,
    Bar
}

class Point {
    x: int,
    y: int

    function new(x: int, y: int) -> Point throws never {
        Point { x, y }
    }
}

test "dotted map keys resolve enum variants rather than strings" {
    let values = {
        KeyKind.Foo: 1,
        KeyKind.Bar: 2,
    };
    assert.equal(values[KeyKind.Foo], 1);
    assert.equal(values[KeyKind.Bar], 2);
    assert.equal(values.keys(), [KeyKind.Foo, KeyKind.Bar])
}

test "bare map keys resolve variables and quoted keys remain strings" {
    let key = "resolved";
    let values = { key: 1, "key": 2 };
    assert.equal(values["resolved"], 1);
    assert.equal(values["key"], 2)
}

test "map keys accept calls arithmetic indexing and constructors" {
    let integers = [3];
    let numbers = {
        1 + 1: "sum",
        integers[0]: "index",
        int.parse("4"): "call",
    };
    assert.equal(numbers[2], "sum");
    assert.equal(numbers[3], "index");
    assert.equal(numbers[4], "call");
    let points = { Point { x: 1, y: 2 }: "constructor" };
    assert.equal(points[Point.new(1, 2)], "constructor")
}

class Counter {
    value: int

    function next(self) -> int throws never {
        self.value += 1;
        self.value
    }
}

test "map key and value expressions evaluate once in source order" {
    let counter = Counter { value: 0 };
    let values = {
        counter.next(): counter.next(),
        counter.next(): counter.next(),
    };
    assert.equal(values.keys(), [1, 3]);
    assert.equal(values.values(), [2, 4]);
    assert.equal(counter.value, 4)
}

test "map shorthand is rejected while class field shorthand remains valid" {
    let x = 2;
    let y = 3;
    assert.equal(Point { x, y }, Point.new(2, 3));
    root.compiler.AssertHasCode({
        "shorthand.baml": `
            function bad() -> map<string, int> {
                let key = 1;
                map { key }
            }
        `,
    }, "E0010")
}
