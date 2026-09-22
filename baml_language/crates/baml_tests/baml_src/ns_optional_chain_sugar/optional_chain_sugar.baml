// The `to_string` / `to_json` sugars (`string.from(x)` / `json.from(x)`) apply
// through an optional chain exactly as they do on a plain receiver: `x?.m()`
// null-tests `x` and then lowers `x.m()` on the non-null path, and that path
// takes the sugar road when no real `m` resolves. Before this pin the chained
// spelling compiled and then failed at run time, because lowering re-read the
// chained node and missed the sugar shape.

class Plain {
    n int
}

class Named {
    n int

    implements baml.ToString {
        function to_string(self) -> string throws never {
            "named:" + self.n.to_string()
        }
    }
}

test "to_string_sugar_through_an_optional_chain" {
    let some: Plain? = Plain { n: 7 };
    let none: Plain? = null;
    assert.is_true(some?.to_string() != null);
    assert.is_true(none?.to_string() == null)
}

test "a_real_to_string_still_wins_through_an_optional_chain" {
    let some: Named? = Named { n: 3 };
    assert.equal(some?.to_string(), "named:3")
}

test "to_json_sugar_through_an_optional_chain" {
    let some: Plain? = Plain { n: 7 };
    let none: Plain? = null;
    assert.is_true(some?.to_json() != null);
    assert.is_true(none?.to_json() == null)
}
