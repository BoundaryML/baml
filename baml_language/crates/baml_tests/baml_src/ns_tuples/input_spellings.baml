test "tuple_json_parenthesized_input" {
    let pair = baml.json.decode<(int, string)>(`(1, "a")`);
    assert.equal(pair, (1, "a"));
    assert.equal(baml.json.stringify(pair), `[1,"a"]`);
    assert.equal(baml.json.decode<(int,)>(`(2,)`), (2,));
    assert.equal(baml.json.decode<((int,), string)>(`((2,), "a")`), ((2,), "a"));
    assert.equal(baml.json.decode<(int, TpjOverride)>(`(1, {"n": 1})`).1.n, 42);
    assert.equal(baml.json.decode<(int,) | (int, string)>(`(1, "a")`), (1, "a"));
    assert.equal(baml.json.decode<(int, string) | int[]>(`(1, "a")`), (1, "a"));
    assert.equal(baml.json.decode<(string, int)>(`("é (a) [b]", 2)`), ("é (a) [b]", 2));
    assert.equal(
        tpj_decode_error(`{"pair": (1,)}`),
        ("expected an array of 2 elements, got 1", `["pair"]`),
    );
    assert.equal(tpj_decode_error(`{"pair": (1, 2)}`), ("expected string", `["pair"][1]`));
    assert.is_true(
        reflect.Type.of_value(baml.json.decode<(int?, string)>(`(1, "a")`))
            == reflect.Type.of<(int?, string)>(),
    )
}

test "parenthesized_open_data_does_not_brand_lists_as_tuples" {
    let value = baml.json.decode<unknown>(`(1, 2)`);
    assert.equal(reflect.Type.of_value(value).as_tuple(), null)
}

test "tuple_json_parentheses_do_not_default_or_evaluate" {
    for (let input in [`[1]`, `(1,)`, `(1, null, 3)`]) {
        let failed = baml.json.decode<(int, string?)>(input) catch (e) {
            baml.json.DecodeError => null,
        };
        assert.equal(failed, null)
    }
    for (let input in [`(1)`, `(1,]`, `(1 + 2, "a")`, `(tp_pair(), "a")`]) {
        let failed = baml.json.decode<(int, string)>(input) catch (e) {
            baml.json.ParseError => null,
        };
        assert.equal(failed, null)
    }
}

test "tuple_sap_parenthesized_input" {
    assert.equal(baml.sap.parse<(int, string)>(`(1, "a")`), (1, "a"));
    assert.equal(baml.sap.parse<(int, string?)>(`(1,)`), (1, null));
    assert.equal(baml.sap.parse<(int, string)>(`(1, "a", true)`), (1, "a"))
}

test "tuples_of_null_are_data_for_output_schemas" {
    assert.is_true(TrRender@render_prompt<(null,)>().text().includes("[null]"));
    let type Empty = reflect.Tuple.new([]);
    assert.is_true(TrRender@render_prompt<Empty>().text().includes("[]"));
    assert.equal(baml.json.stringify(baml.json.decode<Empty>(`()`)), "[]")
}
