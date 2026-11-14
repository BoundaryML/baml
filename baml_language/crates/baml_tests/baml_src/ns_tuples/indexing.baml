class TpiName { name: string }
class TpiCounter { n: int }

function tpi_counted(counter: TpiCounter) -> (int, string)? {
    counter.n += 1;
    (counter.n, "x")
}

function tpi_optional(t: (int, string)?) -> string? { t?.1 }
function tpi_nullable_element(t: (TpiName?, int)) -> string? { t.0?.name }
function tpi_nested(t: ((int,)?, int)) -> int? { t.0?.0 }

function tpi_child_mutation() -> (int[], string) {
    let tuple = ([1], "a");
    tuple.0.push(2);
    tuple.0[0] = 3;
    tuple
}

function tpi_nested_slots(t: ((int, string), bool)) -> string { t.0.1 }
function tpi_narrow_union(t: (int, string) | (float, string)) -> string {
    match t {
        let ints: (int, string) => ints.1,
        let floats: (float, string) => floats.1,
    }
}

class TpiFields { first: int, second: string }
function tpi_fields_from_tuple(t: (int, string)) -> TpiFields {
    TpiFields { first: t.0, second: t.1 }
}

test "tuple_dot_access" {
    let counter = TpiCounter { n: 0 };
    assert.equal(tpi_counted(counter)?.0, 1)
    assert.equal(counter.n, 1)
    assert.equal(tpi_optional((1, "a")), "a")
    assert.equal(tpi_optional(null), null)
    assert.equal(tpi_nullable_element((TpiName { name: "n" }, 1)), "n")
    assert.equal(tpi_nullable_element((null, 1)), null)
    assert.equal(tpi_nested(((7,), 1)), 7)
    assert.equal(tpi_nested((null, 1)), null)
    assert.equal(tpi_child_mutation(), ([3, 2], "a"))
    assert.equal(tpi_nested_slots(((1, "a"), true)), "a")
    assert.equal(tpi_narrow_union((1, "a")), "a")
    assert.equal(tpi_fields_from_tuple((1, "a")), TpiFields { first: 1, second: "a" })
}
