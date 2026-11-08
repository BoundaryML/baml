class RuntimeListBox {
    values: int[],
    rows: int[][],
    flags: bool[],
    points: float[],
}

class RuntimeListItem {
    value: int,
}

function RuntimeLists() -> int {
    let xs: int[] = [];
    for (let i = 0; i < 129; i += 1) {
        xs.push(i);
    }
    let alias = xs;
    xs[-1] = 900;
    let copy = xs.slice(-3, 200).reverse();
    let rows: int[][] = [xs, copy];
    rows[1].push(700);
    let flags = [true, false];
    flags[1] = true;
    let points = [1.5, 2.5];
    points.push(3.5);
    points[0] = -0.5;
    let box = RuntimeListBox { values: alias, rows: rows, flags: flags, points: points };
    let items: RuntimeListItem[] = [RuntimeListItem { value: 42 }];
    items.push(RuntimeListItem { value: 43 });
    items[-1].value = 44;
    let tagged: (int | string | bool | null)[] = [1, "two", true, null];
    tagged.push("five");
    tagged[0] = false;
    let strings = ["a", "b"];
    strings[0] = "c";
    strings.unshift("d");
    strings.remove_at(-1);
    let joined = strings.join(":");
    let total = 0;
    for (let row in box.rows) {
        for (let value in row) {
            total += value;
        }
    }
    if (box.flags[1] && box.points[0] < 0.0 && joined == "d:c") {
        total + items[1].value + tagged.length() + box.values[-1]
    } else {
        -1
    }
}

function RuntimeListBounds() -> int {
    let xs = [10, 20];
    xs[2]
}

function RuntimeListStoreBounds() -> int {
    let xs = ["a"];
    xs[-2] = "bad";
    0
}

function RuntimeListJson() -> int {
    let rows = baml.json.decode<int[][]>("[[1,2],[3,4]]");
    let result = 0;
    for (let row in rows) {
        for (let value in row) {
            result += value;
        }
    }
    result
}

function RuntimeListJsonField() -> int {
    let box = baml.json.decode<RuntimeListBox>(
        "{\"values\":[1,2],\"rows\":[[3]],\"flags\":[true],\"points\":[1.5]}",
    );
    box.values[1] + box.rows[0][0]
}

function RuntimeListJsonError() -> int {
    let rows = baml.json.decode<int[][]>("[[1],[2,\"bad\"]]");
    rows.length()
}

function RuntimeListJsonOptional() -> int {
    let values = baml.json.decode<(int | null)[]>("[1,null,2]");
    let encoded = baml.json.stringify(values);
    if (encoded == "[1,null,2]") {
        values.length()
    } else {
        -1
    }
}

function RuntimeListJsonOptionalError() -> int {
    let values = baml.json.decode<(int | null)[]>("[1,\"bad\"]");
    values.length()
}

function RuntimeListBuiltins() -> int {
    let values = baml.Array.filled(4, 7);
    values.insert(3, -1);
    values.splice(1, 2, values);
    values.unshift(2);
    values.pop();
    values.shift();
    let text = values.join(":");
    if (values.includes(3) && values.index_of(3) != null && values.last_index_of(7) != null) {
        text.length() + values.length()
    } else {
        -1
    }
}

function RuntimeListInsertError() -> int {
    let values = [1, 2];
    values.insert(3, -3)
}

function RuntimeListMaps() -> int {
    let first: map<string, int> = {};
    first.set("x", 42);
    let maps: map<string, int>[] = [first];
    maps.push(first);
    let nested = [maps];
    nested[0][1].set("y", 3);
    let total = 0;
    for (let m in nested[0]) {
        for (let v in m.values()) {
            total += v;
        }
    }
    total
}

class RuntimeListNode {
    value: int,
    children: RuntimeListNode[]?,
}

function RuntimeListRecursiveJson() -> int {
    let nodes = baml.json.decode<RuntimeListNode[]>(
        "[{\"value\":1,\"children\":[{\"value\":2,\"children\":null}]}]",
    );
    let text = baml.json.stringify(nodes);
    if (text == "[{\"value\":1,\"children\":[{\"value\":2,\"children\":null}]}]") {
        nodes[0].value
    } else {
        -1
    }
}

function RuntimeListLiteralJson() -> int {
    let values = baml.json.decode<(1 | true | "yes")[]>("[1,true,\"yes\"]");
    values.length()
}

function RuntimeListLiteralJsonError() -> int {
    let values = baml.json.decode<(1 | true | "yes")[]>("[2]");
    values.length()
}

function RuntimeListJsonFieldError() -> int {
    let value = baml.json.decode<RuntimeListBox>(
        "{\"values\":[1,\"bad\"],\"rows\":[],\"flags\":[],\"points\":[]}",
    );
    value.values.length()
}

function RuntimeListReductions() -> int {
    let floats = [3.0, 1.0, 4.0, 2.0];
    let sorted = baml._rust_sort(floats);
    if (baml._sum_float(floats) == 10.0 && baml._median_float(floats) == 2.5 && sorted[0] == 1.0) {
        baml._sum_int([1, 2, 3])
    } else {
        -1
    }
}

function RuntimeListReductionOverflow() -> int {
    baml._sum_int([4611686018427387903, 1, -1])
}

function RuntimeListMedianError() -> int {
    let floats: float[] = [];
    let value = baml._median_float(floats);
    0
}

function RuntimeListStructuralJoin() -> int {
    let rows = [[1, 2], [3]];
    let classes = [RuntimeListItem { value: 42 }];
    let first: map<string, int> = {};
    first.set("k", 5);
    let maps = [first];
    let optional: string?[] = ["a", null, "b"];
    if (
        rows.join(";") == "[1, 2];[3]"
            && classes.join(",") == "RuntimeListItem { value: 42 }"
            && maps.join(" ") == "{\"k\": 5}"
            && optional.join(":") == "a:null:b"
    ) {
        1
    } else {
        -1
    }
}

function RuntimeListFloatSpecials() -> int {
    let nan = 0.0 / 0.0;
    // Obtain zero at runtime: the VM constant pool coalesces signed-zero
    // literals, which is covered separately below.
    let zero = [0.0][0];
    let xs = [nan, 2.0, -zero, zero, -1.0];
    let sorted = baml._rust_sort(xs);
    if (
        xs.includes(nan)
            && sorted[0] == -1.0
            && sorted[-1] == nan
            && sorted.join(",") == "-1.0,-0.0,0.0,2.0,NaN"
    ) {
        1
    } else {
        -1
    }
}

function RuntimeListJsonNonFinite() -> int {
    let values = [1.0, 1.0 / 0.0];
    let text = baml.json.stringify(values);
    0
}

function NegativeListConstants() -> string {
    [-0.0, 0.0].join(",")
}
function RuntimeListFloatConstants() -> int {
    let positive_first = [0.0, -0.0].join(",");
    if (positive_first == "0.0,0.0" && NegativeListConstants() == "-0.0,-0.0") { 1 } else { -1 }
}
