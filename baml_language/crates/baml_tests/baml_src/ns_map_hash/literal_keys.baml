test "integer_literal_keys" {
    let values: map<int, string> = { 1: "positive", -1: "negative" };
    assert.equal(values.get(1), "positive");
    assert.equal(values.get(-1), "negative");
}

test "boolean_literal_keys" {
    let values: map<bool, string> = { true: "yes", false: "no" };
    assert.equal(values.get(true), "yes");
    assert.equal(values.get(false), "no");
}

test "float_literal_keys" {
    let values: map<float, string> = { 1.5: "positive", -1.5: "negative" };
    assert.equal(values.get(1.5), "positive");
    assert.equal(values.get(-1.5), "negative");
}

test "nullable_literal_keys" {
    let values: map<int?, string> = { null: "none", 1: "one" };
    assert.equal(values.get(null), "none");
    assert.equal(values.get(1), "one");
}

test "computed_keys" {
    let key = 4;
    let values: map<int, string> = { (key): "four", (key + 1): "five" };
    assert.equal(values.get(4), "four");
    assert.equal(values.get(5), "five");
}

test "identifier_keys_remain_strings" {
    let name = "computed";
    let values: map<string, int> = { name: 1, "quoted": 2, (name): 3 };
    assert.equal(values.get("name"), 1);
    assert.equal(values.get("quoted"), 2);
    assert.equal(values.get("computed"), 3);
}
