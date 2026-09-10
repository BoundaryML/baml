function empty() -> map<string, int> {
    return map {};
}

function nested() -> map<string, int[]> {
    return map {values: []};
}

function fail() -> void throws map<string, int> {
    throw map {};
}

test "explicit_maps_construct_and_mutate" {
    let explicit: map<string, int> = map {};
    let implicit: map<string, int> = {};
    explicit["answer"] = 42;
    implicit["answer"] = 42;
    assert.equal(explicit, implicit)
    assert.equal(explicit["answer"], 42)
}

test "explicit_maps_keys_shorthand_and_nesting" {
    let answer = 42;
    let explicit = map {answer, other: 7, "quoted-key": 9};
    let implicit = {answer, other: 7, "quoted-key": 9};
    assert.equal(explicit, implicit)
    assert.equal(explicit["answer"], 42)
    assert.equal(explicit["other"], 7)
    assert.equal(explicit["quoted-key"], 9)
    assert.equal(nested()["values"].length(), 0)
}

test "explicit_maps_return_and_throw" {
    let result = empty();
    assert.equal(result.length(), 0)
    let caught = fail() catch (e) {
        map<string, int> => e.length(),
    };
    assert.equal(caught, 0)
}

function blockValue() -> int {
    let value = { let result = 42; result };
    if (true) {}
    value
}

class Box {
    value int
}

test "explicit_maps_do_not_change_blocks_or_classes" {
    let box = Box {value: 42};
    assert.equal(box.value, blockValue())
}
