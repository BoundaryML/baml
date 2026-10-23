type Numbers = -1 | 7n | "tag";

type GroupedArray = (int | string)[]?;

type TrailingCallback = (int,) -> int throws never;

type QuotedDelimiterCallback = ("(", ")", "{") -> string throws never;

type True = true;

type False = false;

type Either = true | false;

type Grouped = ((int | string));

type Callback = (x?: int) -> string throws never;

function literal_true() -> True throws never {
    true
}

function literal_false() -> False throws never {
    false
}

function either(value: Either) -> bool throws never {
    value
}

function nested(value: map<string, (true | false)[]>) -> bool {
    value["items"][0]
}

function grouped(value: Grouped) -> Grouped throws never {
    value
}

function callback(value: Callback) -> string {
    value()
}

function optional_callback(x: int = 1) -> string throws never {
    "callable"
}

test "literal types and parenthesized unions retain their behavior" {
    assert.equal(literal_true(), true);
    assert.equal(literal_false(), false);
    assert.equal(either(true), true);
    assert.equal(either(false), false);
    assert.equal(nested({ "items": [false, true] }), false);
    assert.equal(grouped(7), 7);
    assert.equal(grouped("grouped"), "grouped");
    assert.equal(callback(optional_callback), "callable")
}

interface View<T> {
    function echo<U>(self, value: U) -> U throws never;
}

class Viewer {
    implements View<map<string, int>> {
        function echo<U>(self, value: U) -> U throws never {
            value
        }
    }
}

interface I {}

class C {
    implements I {}
}

function accepts_i(value: I) -> int throws never {
    7
}

test "single upcast targets retain nested generics and later method arguments" {
    assert.equal(accepts_i(C {}.as<I>), 7);
    assert.equal(Viewer {}.as<View<map<string, int>>>.echo<bool>(true), true);
    assert.equal(Viewer {}.as<View<map<string, int>>,>.echo<string>("nested"), "nested")
}
