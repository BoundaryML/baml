// A class field typed by an alias of a class is read and written through a
// chained path (`holder.box.id`) the same way as a field typed by the class
// written out. The cases cover aliases whose expansion reaches a recursive
// alias, which the runtime type keeps as an alias name: `baml.json.json`, a
// user-defined recursive union, a recursive alias reached through a second
// alias, and an alias that is itself recursive (`type Tree = Box<Tree[]>`).
//
// A chained path that does not resolve the alias to its class reads the field
// as a map key ("expected map, got instance" at run time) and has no place to
// assign to (an internal compiler error).

class Box<T> {
    id: int,
    value: T,
}

type MyJson = int | string | MyJson[] | map<string, MyJson>;

type JsonList = baml.json.json[];

type IntBox = Box<int>;

type JsonBox = Box<baml.json.json>;

type MyJsonBox = Box<MyJson>;

type JsonListBox = Box<JsonList>;

type Tree = Box<Tree[]>;

class IntBoxHolder {
    box: IntBox,
}

class JsonBoxHolder {
    box: JsonBox,
}

class MyJsonBoxHolder {
    box: MyJsonBox,
}

class JsonListBoxHolder {
    box: JsonListBox,
}

class TreeHolder {
    box: Tree,
}

class Outer {
    holder: JsonBoxHolder,
}

function json_box() -> Box<baml.json.json> {
    Box<baml.json.json> {
        id: 7,
        value: { "a": 1 },
    }
}

function tree(id: int, children: Tree[]) -> Tree {
    Box<Tree[]> {
        id: id,
        value: children,
    }
}

function read_id_through_parameter(holder: JsonBoxHolder) -> int {
    holder.box.id
}

function write_id_through_parameter(holder: JsonBoxHolder, id: int) -> int {
    holder.box.id = id;
    holder.box.id
}

test "chained_read_through_an_alias_of_a_class" {
    let holder = IntBoxHolder {
        box: Box<int> { id: 7, value: 1 },
    };
    assert.equal(holder.box.id, 7);
    assert.equal(holder.box.value, 1)
}

test "chained_write_through_an_alias_of_a_class" {
    let holder = IntBoxHolder {
        box: Box<int> { id: 7, value: 1 },
    };
    holder.box.id = 8;
    holder.box.value = 2;
    let box = holder.box;
    assert.equal(box.id, 8);
    assert.equal(box.value, 2)
}

test "chained_read_through_an_alias_of_a_class_of_json" {
    let holder = JsonBoxHolder { box: json_box() };
    assert.equal(holder.box.id, 7)
}

test "chained_read_of_the_generic_field_through_an_alias_of_a_class_of_json" {
    let holder = JsonBoxHolder { box: json_box() };
    assert.equal(holder.box.value, { "a": 1 })
}

test "chained_write_through_an_alias_of_a_class_of_json" {
    let holder = JsonBoxHolder { box: json_box() };
    holder.box.id = 8;
    holder.box.value = [1, "two"];
    let box = holder.box;
    assert.equal(box.id, 8);
    assert.equal(box.value, [1, "two"])
}

test "chained_compound_assignment_through_an_alias_of_a_class_of_json" {
    let holder = JsonBoxHolder { box: json_box() };
    holder.box.id += 1;
    let box = holder.box;
    assert.equal(box.id, 8)
}

test "chained_read_through_a_parameter" {
    assert.equal(
        read_id_through_parameter(JsonBoxHolder { box: json_box() }),
        7,
    )
}

test "chained_write_through_a_parameter" {
    assert.equal(
        write_id_through_parameter(JsonBoxHolder { box: json_box() }, 8),
        8,
    )
}

test "four_segment_read_and_write_through_an_alias_of_a_class_of_json" {
    let outer = Outer {
        holder: JsonBoxHolder { box: json_box() },
    };
    assert.equal(outer.holder.box.id, 7);
    outer.holder.box.id = 8;
    assert.equal(outer.holder.box.id, 8)
}

test "chained_read_through_an_alias_of_a_class_of_a_recursive_union" {
    let holder = MyJsonBoxHolder {
        box: Box<MyJson> {
            id: 7,
            value: [1, "two"],
        },
    };
    assert.equal(holder.box.id, 7);
    assert.equal(holder.box.value, [1, "two"])
}

test "chained_write_through_an_alias_of_a_class_of_a_recursive_union" {
    let holder = MyJsonBoxHolder {
        box: Box<MyJson> { id: 7, value: 1 },
    };
    holder.box.id = 8;
    holder.box.value = { "k": [2] };
    let box = holder.box;
    assert.equal(box.id, 8);
    assert.equal(box.value, { "k": [2] })
}

test "chained_read_through_an_alias_that_reaches_json_through_a_second_alias" {
    let holder = JsonListBoxHolder {
        box: Box<JsonList> {
            id: 7,
            value: [1, null],
        },
    };
    assert.equal(holder.box.id, 7);
    assert.equal(holder.box.value.length(), 2)
}

test "chained_write_through_an_alias_that_reaches_json_through_a_second_alias" {
    let holder = JsonListBoxHolder {
        box: Box<JsonList> { id: 7, value: [] },
    };
    holder.box.id = 8;
    let box = holder.box;
    assert.equal(box.id, 8)
}

test "chained_read_through_a_self_recursive_alias" {
    let holder = TreeHolder {
        box: tree(7, [tree(1, [])]),
    };
    assert.equal(holder.box.id, 7);
    assert.equal(holder.box.value.length(), 1);
    assert.equal(holder.box.value[0].id, 1)
}

test "chained_write_through_a_self_recursive_alias" {
    let holder = TreeHolder { box: tree(7, []) };
    holder.box.id = 8;
    holder.box.value = [tree(2, [])];
    let box = holder.box;
    assert.equal(box.id, 8);
    assert.equal(box.value[0].id, 2)
}
