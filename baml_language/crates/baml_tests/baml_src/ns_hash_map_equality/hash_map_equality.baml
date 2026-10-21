class CollisionKey {
    id: int

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            0.hash(state)
        }
    }
}

function digest<T extends baml.Hash>(value: T) -> int throws never {
    let state = baml.hash.DefaultHasher.new();
    value.hash(state);
    state.finish()
}

test "map equality and hashing ignore insertion order with colliding keys" {
    let first: map<CollisionKey, string> = {};
    first.set(CollisionKey { id: 1 }, "one");
    first.set(CollisionKey { id: 2 }, "two");
    let second: map<CollisionKey, string> = {};
    second.set(CollisionKey { id: 2 }, "two");
    second.set(CollisionKey { id: 1 }, "one");
    assert.is_true(first == second);
    assert.equal(digest(first), digest(second));
    second.set(CollisionKey { id: 2 }, "different");
    assert.is_true(first != second)
}

class IdentityKey {
    id: int,
    label: string

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.id == other.id
        }
    }

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            self.id.hash(state)
        }
    }
}

test "map equality and hashing honor custom key equality" {
    let first: map<IdentityKey, int> = {};
    first.set(IdentityKey { id: 1, label: "first" }, 10);
    let second: map<IdentityKey, int> = {};
    second.set(IdentityKey { id: 1, label: "second" }, 10);
    assert.is_true(first == second);
    assert.equal(digest(first), digest(second));
    let outer: map<map<IdentityKey, int>, string> = {};
    outer.set(first, "found");
    assert.equal(outer[second], "found")
}

class RecursiveValue {
    children: map<int, RecursiveValue>
}

test "map value equality retains cycle detection" {
    let first = RecursiveValue { children: {} };
    first.children.set(1, first);
    let second = RecursiveValue { children: {} };
    second.children.set(1, second);
    assert.is_true(first == second);
    second.children.set(2, second);
    assert.is_true(first != second)
}

class CyclicKey {
    owner: map<CyclicKey, int>

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            0.hash(state)
        }
    }
}

test "map key equality retains ancestor cycle detection" {
    let first: map<CyclicKey, int> = {};
    first.set(CyclicKey { owner: first }, 1);
    let second: map<CyclicKey, int> = {};
    second.set(CyclicKey { owner: second }, 1);
    assert.is_true(first == second);
    second.set(second.keys()[0], 2);
    assert.is_true(first != second)
}

class UpdatingKey {
    id: int,
    owner: map<UpdatingKey, int>

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            self.id.hash(state)
        }
    }

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.owner.set(self, 1);
            self.id == other.id
        }
    }
}

test "value updates from key equality do not restart lookup" {
    let entries: map<UpdatingKey, int> = {};
    let stored = UpdatingKey { id: 1, owner: entries };
    entries.set(stored, 0);
    let query = UpdatingKey { id: 1, owner: entries };
    assert.equal(entries.get(query), 1);
    assert.is_true(entries.has(query));
    assert.equal(entries.get_or_insert(query, 2), 1);
    assert.equal(entries.delete(query), 1)
}

class NestedKey {
    children: map<NestedKey, int>

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            0.hash(state)
        }
    }
}

test "deep map key equality uses an iterative worklist" {
    let first: map<NestedKey, int> = {};
    let second: map<NestedKey, int> = {};
    let i = 0;
    while (i < 2048) {
        let a: map<NestedKey, int> = {};
        let b: map<NestedKey, int> = {};
        a.set(NestedKey { children: first }, i);
        b.set(NestedKey { children: second }, i);
        first = a;
        second = b;
        i += 1;
    }
    assert.is_true(first == second)
}

class YieldingKey {
    id: int

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            0.hash(state)
        }
    }

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            baml.sys.collect_garbage();
            self.id == other.id
        }
    }
}

test "failed yielding key candidates preserve later comparisons across GC" {
    let first: map<YieldingKey, int> = {};
    first.set(YieldingKey { id: 1 }, 10);
    first.set(YieldingKey { id: 2 }, 20);
    let second: map<YieldingKey, int> = {};
    second.set(YieldingKey { id: 2 }, 20);
    second.set(YieldingKey { id: 1 }, 10);
    assert.is_true(first == second);
    second.set(YieldingKey { id: 2 }, 99);
    assert.is_true(first != second)
}
