// `T[].join(separator)` renders each element as `string.from` does, with
// `separator` between them, so `ToString` overrides apply at every depth.

class JoinMoney {
    cents: int,
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "$" + self.cents.to_string()
        }
    }
}

class JoinPoint {
    x: int,
    y: int,
}

class JoinWallet {
    owner: string,
    money: JoinMoney,
}

class JoinLogged {
    name: string,
    log: string[],
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self.log.push(self.name);
            self.name
        }
    }
}

enum JoinColor {
    Red,
    Green,
}

function join_generic<T>(values: T[], separator: string) -> string {
    values.join(separator)
}

test "join_counts_separators" {
    let empty: string[] = [];
    assert.equal(empty.join(","), "");
    assert.equal(["a"].join(","), "a");
    assert.equal([7].join(","), "7");
    assert.equal(["a", "b", "c"].join(","), "a,b,c");
    assert.equal(["a", "b", "c"].join(""), "abc");
    assert.equal(["a", "b"].join(" -> "), "a -> b");
    assert.equal(["", "", ""].join("|"), "||")
}

test "join_strings_are_bare" {
    assert.equal(["héllo", "😀", "", "say \"hi\""].join("—"), "héllo—😀——say \"hi\"");
    assert.equal(join_generic(["x", "y"], "+"), "x+y")
}

test "join_primitives" {
    assert.equal([1, -2, int.max_value()].join(", "), "1, -2, 4611686018427387903");
    assert.equal([1.0, 0.5, -0.0, float.nan()].join(" "), "1.0 0.5 -0.0 NaN");
    assert.equal([true, false].join("||"), "true||false");
    let mixed: (int | string | float | bool | null)[] = [1, "a", 2.5, true, null];
    assert.equal(mixed.join(","), "1,a,2.5,true,null");
    assert.equal(join_generic([1, 2, 3], ""), "123")
}

test "join_nested_values_render_structurally" {
    assert.equal([[1, 2], [3]].join(";"), "[1, 2];[3]");
    assert.equal([["a", "b"]].join(";"), "[\"a\", \"b\"]");
    assert.equal([{ "k": 1 }, { "k": 2 }].join(" "), "{\"k\": 1} {\"k\": 2}");
    assert.equal([JoinColor.Red, JoinColor.Green].join(","), "Red,Green")
}

test "join_honors_to_string_overrides" {
    assert.equal([JoinMoney { cents: 5 }, JoinMoney { cents: 70 }].join(" + "), "$5 + $70");
    assert.equal(join_generic([JoinMoney { cents: 1 }], ","), "$1")
}

test "join_renders_structural_classes" {
    assert.equal(
        [JoinPoint { x: 1, y: 2 }, JoinPoint { x: 3, y: 4 }].join(" "),
        "JoinPoint { x: 1, y: 2 } JoinPoint { x: 3, y: 4 }",
    );
    assert.equal(
        [JoinWallet { owner: "ann", money: JoinMoney { cents: 9 } }].join(","),
        "JoinWallet { owner: \"ann\", money: $9 }",
    );
    let mixed: (JoinMoney | JoinPoint | string)[] = [JoinMoney { cents: 2 }, JoinPoint { x: 0, y: 1 }, "s"];
    assert.equal(mixed.join("|"), "$2|JoinPoint { x: 0, y: 1 }|s")
}

test "join_calls_overrides_in_element_order" {
    let log: string[] = [];
    let values = [
        JoinLogged { name: "a", log: log },
        JoinLogged { name: "b", log: log },
        JoinLogged { name: "c", log: log },
    ];
    assert.equal(values.join(","), "a,b,c");
    assert.equal(log, ["a", "b", "c"])
}

test "join_long_array" {
    let values: int[] = [];
    for (let i = 0; i < 1000; i += 1) {
        values.push(i);
    }
    let joined = values.join(",");
    assert.equal(joined.length(), 3889);
    assert.equal(joined.slice(0, 8), "0,1,2,3,");
    assert.equal(joined.slice(-7, joined.length()), "998,999")
}

class JoinCollecting {
    n: int,
    implements baml.ToString {
        function to_string(self) -> string throws never {
            baml.sys.collect_garbage();
            "c" + self.n.to_string()
        }
    }
}

class JoinBox {
    inner: JoinCollecting,
}

function join_by_hand(
    values: (JoinCollecting | JoinBox | JoinPoint | string)[],
    separator: string,
) -> string {
    let out = "";
    for (let i = 0; i < values.length(); i += 1) {
        if (i > 0) {
            out = out + separator;
        }
        out = out + string.from(values[i]);
    }
    out
}

test "join_survives_collections_inside_overrides" {
    let values: (JoinCollecting | JoinBox | JoinPoint | string)[] = [];
    for (let i = 0; i < 4; i += 1) {
        values.push(JoinCollecting { n: i });
        values.push(JoinBox { inner: JoinCollecting { n: -i } });
        values.push(JoinPoint { x: i, y: i + 1 });
        values.push(`s${i}`);
    }
    let joined = values.join(", ");
    assert.equal(joined, join_by_hand(values, ", "));
    assert.equal(joined.slice(0, 51), "c0, JoinBox { inner: c0 }, JoinPoint { x: 0, y: 1 }")
}
