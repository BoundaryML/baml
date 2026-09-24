// Concurrent container-mutation regressions. Exact results are intentionally
// nondeterministic; the contract is that no VM/container corruption occurs and
// the surviving length/value stays within the operation's mathematical bound.

function sar_push_ints(values: int[], n: int) -> int {
    let i = 0;
    while (i < n) {
        values.push(i);
        i += 1
    }
    values.length()
}

function sar_pop_ints(values: int[], n: int) -> int {
    let i = 0;
    while (i < n) {
        values.pop();
        i += 1
    }
    0
}

function sar_read_ints(values: int[], n: int) -> int {
    let i = 0;
    let total = 0;
    while (i < n) {
        if (values.length() > 0) {
            total += values[0]
        }
        i += 1
    }
    total
}

function sar_array_push() -> int {
    let values: int[] = [];
    let a = spawn { sar_push_ints(values, 200) };
    let b = spawn { sar_push_ints(values, 200) };
    let c = spawn { sar_push_ints(values, 200) };
    let d = spawn { sar_push_ints(values, 200) };
    (await a) + (await b) + (await c) + (await d);
    values.length()
}

test "racing_array_push_does_not_crash" {
    let length = sar_array_push();
    assert.is_true(length >= 0 && length <= 800)
}

function sar_array_push_pop() -> int {
    let values = [1, 2, 3, 4, 5];
    let push = spawn { sar_push_ints(values, 500) };
    let pop = spawn { sar_pop_ints(values, 500) };
    (await push) + (await pop);
    values.length()
}

test "racing_array_push_pop_does_not_crash" {
    let length = sar_array_push_pop();
    assert.is_true(length >= 0 && length <= 505)
}

function sar_array_read_grow() -> int {
    let values = [1];
    let grow = spawn { sar_push_ints(values, 1000) };
    let read_a = spawn { sar_read_ints(values, 500) };
    let read_b = spawn { sar_read_ints(values, 500) };
    (await grow) + (await read_a) + (await read_b);
    values.length()
}

test "racing_array_index_read_vs_grow_does_not_crash" {
    let length = sar_array_read_grow();
    assert.is_true(length >= 1 && length <= 1001)
}

function sar_set_map(values: map<string, int>, key: string, n: int) -> int {
    let i = 0;
    while (i < n) {
        values.set(key, i);
        i += 1
    }
    0
}

function sar_delete_map(values: map<string, int>, key: string, n: int) -> int {
    let i = 0;
    while (i < n) {
        values.delete(key);
        i += 1
    }
    0
}

function sar_map_set_delete() -> int {
    let values: map<string, int> = {};
    let set_a = spawn { sar_set_map(values, "a", 200) };
    let set_b = spawn { sar_set_map(values, "b", 200) };
    let delete_a = spawn { sar_delete_map(values, "a", 200) };
    (await set_a) + (await set_b) + (await delete_a);
    values.length()
}

test "racing_map_set_delete_does_not_crash" {
    let length = sar_map_set_delete();
    assert.is_true(length >= 0 && length <= 2)
}

function sar_push_bytes(values: uint8array, n: int) -> int {
    let i = 0;
    while (i < n) {
        values.push(i);
        i += 1
    }
    values.length()
}

function sar_pop_bytes(values: uint8array, n: int) -> int {
    let i = 0;
    while (i < n) {
        values.pop();
        i += 1
    }
    0
}

function sar_read_bytes(values: uint8array, n: int) -> int {
    let i = 0;
    let total = 0;
    while (i < n) {
        if (values.length() > 0) {
            total += values[0]
        }
        i += 1
    }
    total
}

function sar_sort_bytes(values: uint8array, n: int) -> int {
    let i = 0;
    while (i < n) {
        values.sort();
        i += 1
    }
    0
}

function sar_bytes_push() -> int {
    let values = b"";
    let a = spawn { sar_push_bytes(values, 200) };
    let b = spawn { sar_push_bytes(values, 200) };
    let c = spawn { sar_push_bytes(values, 200) };
    let d = spawn { sar_push_bytes(values, 200) };
    (await a) + (await b) + (await c) + (await d);
    values.length()
}

test "racing_uint8array_push_does_not_crash" {
    let length = sar_bytes_push();
    assert.is_true(length >= 0 && length <= 800)
}

function sar_bytes_push_pop() -> int {
    let values = b"\x01\x02\x03\x04\x05";
    let push = spawn { sar_push_bytes(values, 500) };
    let pop = spawn { sar_pop_bytes(values, 500) };
    (await push) + (await pop);
    values.length()
}

test "racing_uint8array_push_pop_does_not_crash" {
    let length = sar_bytes_push_pop();
    assert.is_true(length >= 0 && length <= 505)
}

function sar_bytes_read_grow() -> int {
    let values = b"\x01";
    let grow = spawn { sar_push_bytes(values, 1000) };
    let read_a = spawn { sar_read_bytes(values, 500) };
    let read_b = spawn { sar_read_bytes(values, 500) };
    (await grow) + (await read_a) + (await read_b);
    values.length()
}

test "racing_uint8array_index_read_vs_grow_does_not_crash" {
    let length = sar_bytes_read_grow();
    assert.is_true(length >= 1 && length <= 1001)
}

function sar_bytes_sort_grow() -> int {
    let values = b"\x03\x02\x01";
    let push = spawn { sar_push_bytes(values, 300) };
    let sort = spawn { sar_sort_bytes(values, 100) };
    (await push) + (await sort);
    values.length()
}

test "racing_uint8array_sort_vs_grow_does_not_crash" {
    let length = sar_bytes_sort_grow();
    assert.is_true(length >= 3 && length <= 303)
}

function sar_captured_local_increment() -> int {
    let value = 0;
    let a = spawn {
        let i = 0;
        while (i < 400) {
            value += 1;
            i += 1
        }
        0
    };
    let b = spawn {
        let i = 0;
        while (i < 400) {
            value += 1;
            i += 1
        }
        0
    };
    (await a) + (await b);
    value
}

test "racing_captured_local_increment_does_not_crash" {
    let value = sar_captured_local_increment();
    assert.is_true(value >= 0 && value <= 800)
}

class SarCounter {
    value: int,
}

function sar_bump(counter: SarCounter, n: int) -> int {
    let i = 0;
    while (i < n) {
        counter.value += 1;
        i += 1
    }
    counter.value
}

function sar_class_field_increment() -> int {
    let counter = SarCounter { value: 0 };
    let a = spawn { sar_bump(counter, 400) };
    let b = spawn { sar_bump(counter, 400) };
    (await a) + (await b);
    counter.value
}

test "racing_class_field_increment_does_not_crash" {
    let value = sar_class_field_increment();
    assert.is_true(value >= 0 && value <= 800)
}
