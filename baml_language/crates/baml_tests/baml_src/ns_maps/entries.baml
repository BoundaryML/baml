class EntryBox {
    value: int,
}

test "map entries preserve insertion order and declared tuple slots" {
    let values: map<int, string?> = { 2: "two", 1: null };
    let entries = values.entries();
    assert.equal(entries, [(2, "two"), (1, null)]);
    assert.is_true(reflect.Type.of_value(entries) == reflect.Type.of<(int, string?)[]>());
    assert.is_true(reflect.Type.of_value(entries[0]) == reflect.Type.of<(int, string?)>());
    let empty: map<int, EntryBox?> = {};
    assert.is_true(reflect.Type.of_value(empty.entries()) == reflect.Type.of<(int, EntryBox?)[]>())
}

test "map entries snapshot slots and share mutable children across GC" {
    let key = EntryBox { value: 1 };
    let child = EntryBox { value: 10 };
    let values: map<EntryBox, EntryBox?> = {};
    values[key] = child;
    let entries = values.entries();
    values[key] = null;
    values.clear();
    baml.sys.collect_garbage();
    key.value = 2;
    child.value = 20;
    assert.equal(entries[0].0.value, 2);
    assert.equal(entries[0].1?.value, 20);
    assert.is_true(reflect.Type.of_value(entries[0]) == reflect.Type.of<(EntryBox, EntryBox?)>());
    entries.push((EntryBox { value: 3 }, null));
    assert.equal(values.length(), 0)
}

test "map from entries handles duplicate keys in insertion order" {
    let pairs: (int, string?)[] = [(2, "first"), (1, null), (2, "last")];
    let values = baml.Map.from_entries(pairs);
    assert.equal(values.entries(), [(2, "last"), (1, null)]);
    assert.is_true(values.has(1));
    let empty: (int, string?)[] = [];
    let result = baml.Map.from_entries(empty);
    assert.equal(result.length(), 0);
    assert.is_true(reflect.Type.of_value(result) == reflect.Type.of<map<int, string?>>())
}

test "map from entries consumes lazy sources and propagates their errors" {
    let values = baml.Map.from_entries([10, 20].enumerate());
    assert.equal(values.entries(), [(0, 10), (1, 20)]);
    let caught = false;
    {
        baml.Map.from_entries(root.iter.FlakyIterator.new([10, 20], 1).enumerate());
    } catch (e) {
        baml.errors.InvalidArgument => {
            caught = true;
        }
    };
    assert.is_true(caught)
}

test "map from entries uses custom equality and retains the first key" {
    let first = root.map_hash.Key { id: 1, label: "first" };
    let equal = root.map_hash.Key { id: 1, label: "replacement" };
    let pairs: (root.map_hash.Key, int)[] = [(first, 1), (equal, 2)];
    let values: map<root.map_hash.Key, int> = baml.Map.from_entries(pairs);
    baml.sys.collect_garbage();
    assert.equal(values.length(), 1);
    assert.equal(values.entries()[0].0.label, "first");
    assert.equal(values[equal], 2)
}

test "map entry children remain shared when the snapshot escapes to a task" {
    let values: map<int, int[]> = { 1: [10] };
    let entry = values.entries()[0];
    await spawn { entry.1.push(20); };
    assert.equal(values[1], [10, 20])
}
