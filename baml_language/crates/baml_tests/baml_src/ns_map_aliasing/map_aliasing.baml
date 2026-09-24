// Regression coverage for preserving mutable map/list/class identity across
// calls and loop iterations.

function put_map_with_key(m: map<string, int>, key: string) -> int {
    m.set(key, 1);
    return m.length();
}

function map_aliasing_without_caller_read() -> string {
    let m: map<string, int> = {};
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_map_with_key(m, "k" + i.to_string()) catch (e) {
            _ => -1
        };
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

function map_aliasing_with_caller_read() -> string {
    let m: map<string, int> = {};
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_map_with_key(m, "k" + i.to_string()) catch (e) {
            _ => -1
        };
        out = out + n.to_string() + ",";
        i += 1;
    }
    out + " caller=" + m.length().to_string()
}

function put_map_tail(m: map<string, int>, key: string) -> int {
    m.set(key, 1);
    m.length()
}

function map_aliasing_return_phi() -> string {
    let m: map<string, int> = {};
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_map_tail(m, "k" + i.to_string());
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

function put_fresh_map(m: map<string, int>) -> int {
    m.set("k", 1);
    m.length()
}

function map_allocated_inside_loop() -> string {
    let out = "";
    let i = 0;
    while (i < 3) {
        let m: map<string, int> = {};
        let n = put_fresh_map(m);
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

function put_int(xs: int[]) -> int {
    xs.push(1);
    xs.length()
}

function int_list_aliasing() -> string {
    let xs: int[] = [];
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_int(xs);
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

function put_string(xs: string[], value: string) -> int {
    xs.push(value);
    xs.length()
}

function string_list_aliasing() -> string {
    let xs: string[] = [];
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_string(xs, "v" + i.to_string());
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

function put_nested(xs: int[][], value: int) -> int {
    xs.push([value]);
    xs.length()
}

function nested_list_aliasing() -> string {
    let xs: int[][] = [];
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_nested(xs, i);
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

class Counter {
    n: int,
}

function bump(counter: Counter) -> int {
    counter.n = counter.n + 1;
    counter.n
}

function class_field_aliasing() -> string {
    let counter = Counter { n: 0 };
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = bump(counter);
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

class MapBox {
    items: map<string, int>,
}

function put_box(box: MapBox, key: string) -> int {
    box.items.set(key, 1);
    box.items.length()
}

function class_map_aliasing() -> string {
    let box = MapBox { items: {} };
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_box(box, "k" + i.to_string());
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

class Acc {
    items: string[],
}

function put_acc(acc: Acc, value: string) -> int {
    acc.items.push(value);
    acc.items.length()
}

function class_list_aliasing() -> string {
    let acc = Acc { items: [] };
    let out = "";
    let i = 0;
    while (i < 3) {
        let n = put_acc(acc, "v" + i.to_string());
        out = out + n.to_string() + ",";
        i += 1;
    }
    out
}

test "callee_map_mutations_persist_when_caller_does_not_read_map" {
    assert.equal(map_aliasing_without_caller_read(), "1,2,3,")
}

test "callee_map_mutations_persist_when_caller_reads_map" {
    assert.equal(map_aliasing_with_caller_read(), "1,2,3, caller=3")
}

test "map_identity_survives_return_phi_loop_cfg" {
    assert.equal(map_aliasing_return_phi(), "1,2,3,")
}

test "map_allocated_inside_loop_stays_fresh" {
    assert.equal(map_allocated_inside_loop(), "1,1,1,")
}

// The former Rust suite ran this identical program at Opt1 and Opt2. Native
// corpus tests compile at Opt2, so one behavioral oracle replaces the pair.
test "int_list_identity_survives_callee_mutation_in_loop" {
    assert.equal(int_list_aliasing(), "1,2,3,")
}

test "string_list_identity_survives_callee_mutation_in_loop" {
    assert.equal(string_list_aliasing(), "1,2,3,")
}

test "nested_list_identity_survives_callee_mutation_in_loop" {
    assert.equal(nested_list_aliasing(), "1,2,3,")
}

test "class_identity_survives_callee_field_mutation_in_loop" {
    assert.equal(class_field_aliasing(), "1,2,3,")
}

test "class_with_map_identity_survives_callee_mutation_in_loop" {
    assert.equal(class_map_aliasing(), "1,2,3,")
}

test "class_with_list_identity_survives_callee_mutation_in_loop" {
    assert.equal(class_list_aliasing(), "1,2,3,")
}
