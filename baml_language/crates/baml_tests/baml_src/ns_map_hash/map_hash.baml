class Key {
    id: int
    label: string

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            baml.sys.collect_garbage();
            self.id == other.id
        }
    }

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            baml.sys.collect_garbage();
            0.hash(state);
        }
    }
}

test "primitive keys use the same operations as string keys" {
    let m: map<int, string> = { 2: "two", 1: "one" };
    assert.equal(m[2], "two")
    assert.equal(m.set(2, "second"), "two")
    m[3] = "three";
    assert.equal(m.keys(), [2, 1, 3])
    assert.equal(m.values(), ["second", "one", "three"])
    assert.equal(m.get_or_insert(1, "unused"), "one")
    assert.equal(m.get_or_insert(4, "four"), "four")
    assert.equal(m.delete(1), "one")
    assert.equal(m.delete(1), null)
    assert.is_true(!m.has(1))
    assert.equal(m.get(1), null)
    assert.equal(m.keys(), [2, 3, 4])
    let flags: map<bool, int> = { true: 1, false: 2 };
    assert.equal(flags[false], 2)
    let floats: map<float, int> = {};
    floats.set(-0.0, 1);
    assert.equal(floats[0.0], 1)
    floats.set(float.nan(), 2);
    assert.equal(floats[float.parse("NaN")], 2)
}

test "literal duplicate keys replace values without changing order" {
    let m: map<int, string> = { 2: "first", 1: "one", 2: "last" };
    assert.equal(m.keys(), [2, 1])
    assert.equal(m.values(), ["last", "one"])
    let nullable: map<int, string?> = {};
    nullable.set(1, null);
    assert.is_true(nullable.has(1))
    assert.equal(nullable.get_or_insert(1, "unused"), null)
    assert.equal(nullable[1], null)
    let caught = false;
    {
        nullable[2];
    } catch (error) {
        baml.panics.MapKeyNotFound => { caught = true; }
    }
    assert.is_true(caught)
}

test "custom equality collisions preserve the first key and insertion order across GC" {
    let first = Key { id: 1, label: "first" };
    let equal = Key { id: 1, label: "replacement" };
    let second = Key { id: 2, label: "second" };
    let m: map<Key, string> = {};
    assert.equal(m.set(first, "a"), null)
    assert.equal(m.set(second, "b"), null)
    assert.equal(m.set(equal, "updated"), "a")
    assert.equal(m.length(), 2)
    assert.equal(m.keys()[0].label, "first")
    assert.equal(m.keys()[1].label, "second")
    assert.equal(m[equal], "updated")
    assert.equal(m.get_or_insert(equal, "unused"), "updated")
    assert.equal(m.delete(equal), "updated")
    assert.equal(m.keys()[0].id, 2)
    m[equal] = "again";
    assert.equal(m.keys()[1].label, "replacement")
    assert.equal(m.values(), ["b", "again"])
}

class Mutation {
    entries: map<MutatingKey, int>
    clear_on_equals: bool
    clear_on_hash: bool
}

class MutatingKey {
    id: int
    mutation: Mutation

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            if (self.mutation.clear_on_equals) {
                self.mutation.clear_on_equals = false;
                self.mutation.entries.clear();
            }
            baml.sys.collect_garbage();
            self.id == other.id
        }
    }

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            if (self.mutation.clear_on_hash) {
                self.mutation.clear_on_hash = false;
                self.mutation.entries.clear();
            }
            0.hash(state);
        }
    }
}

test "callback mutations invalidate collision snapshots before committing" {
    let mutation = Mutation { entries: {}, clear_on_equals: false, clear_on_hash: false };
    let a = MutatingKey { id: 1, mutation };
    let b = MutatingKey { id: 1, mutation };
    mutation.entries.set(a, 10);
    mutation.clear_on_equals = true;
    assert.equal(mutation.entries.get(b), null)
    assert.equal(mutation.entries.length(), 0)
    mutation.entries.set(a, 20);
    mutation.clear_on_equals = true;
    assert.equal(mutation.entries.set(b, 30), null)
    assert.equal(mutation.entries[b], 30)
    mutation.clear_on_hash = true;
    assert.equal(mutation.entries.get(b), null)
    assert.equal(mutation.entries.length(), 0)
}

test "structural class and array keys retain their key values" {
    let arrays: map<int[], string> = {};
    arrays.set([1, 2], "pair");
    assert.equal(arrays[[1, 2]], "pair")
    assert.equal(arrays.keys(), [[1, 2]])
    let records: map<Record, int> = {};
    records.set(Record { name: "x", count: 1 }, 7);
    assert.equal(records[Record { name: "x", count: 1 }], 7)
}

class Record {
    name: string
    count: int
}
