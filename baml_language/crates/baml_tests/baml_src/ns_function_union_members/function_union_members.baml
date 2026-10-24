// B-1722: a bare non-first callable arm must retain its head, return and
// throws types. These forms are unambiguous under greedy function precedence.
type Value = int | () -> string throws never;

class Holder {
    value: int | () -> string throws never,
}

function h(v: int | () -> string throws never) -> int throws never {
    1
}

function call_value(v: int | () -> string throws never) -> string {
    if let cb: () -> string throws never = v {
        cb()
    } else {
        "int"
    }
}

function identity<T>(v: T) -> T {
    v
}

function make(b: bool) -> int | () -> string throws never {
    if (b) {
        () -> string throws never {
            "returned"
        }
    } else {
        7
    }
}

function read_map(values: map<string, int | () -> string throws never>) -> string {
    call_value(values["v"])
}

function raise(b: bool) -> never throws int | () -> string throws never {
    if (b) {
        throw 7;
    }
    throw () -> string throws never { "thrown" };
}

function catch_value(b: bool) -> string {
    raise(b) catch (e) {
        int => "int",
        () -> string throws never => e(),
    }
}

function return_union(b: bool) -> (() -> string | int throws never) {
    if (b) {
        () -> string throws never {
            "s"
        }
    } else {
        () -> int throws never {
            7
        }
    }
}

test "original bare callable parameter accepts the lambda" {
    assert.equal(
        h(() -> string throws never {
            "s"
        }),
        1,
    );
    assert.equal(h(7), 1);
    assert.equal(
        call_value(v = () -> string throws never {
            "named"
        }),
        "named",
    );
    assert.equal(call_value(7), "int")
}

test "bare callable survives fields aliases and generic arguments" {
    let holder = Holder {
        value: () -> string throws never {
            "field"
        },
    };
    assert.equal(call_value(holder.value), "field");
    let value = identity<Value>(() -> string throws never {
        "alias"
    });
    assert.equal(call_value(value), "alias");
    assert.equal(
        read_map(
            {
                "v": () -> string throws never {
                    "map"
                },
            },
        ),
        "map",
    );
    assert.equal(read_map({ "v": 7 }), "int")
}

test "bare callable survives returns and throws" {
    assert.equal(call_value(make(true)), "returned");
    assert.equal(call_value(make(false)), "int");
    assert.equal(catch_value(false), "thrown");
    assert.equal(catch_value(true), "int")
}

test "function return union keeps greedy precedence" {
    assert.equal(return_union(true)(), "s");
    assert.equal(return_union(false)(), 7)
}

test "let and pattern alternatives keep the callable arm" {
    let value: int | () -> string throws never = () -> string throws never {
        "local"
    };
    assert.equal(call_value(value), "local");
    let unknown_value: unknown = value;
    assert.is_true(unknown_value is int | () -> string throws never);
    assert.is_true(!("s" is int | () -> string throws never));
    assert.equal(
        match (unknown_value) {
            let cb: () -> string throws never => cb(),
            _ => "missed",
        },
        "local",
    )
}
