class RecordingHasher {
    bytes: int[]
    writes: int

    implements baml.hash.Hasher {
        function write(self, bytes: uint8array) -> void throws never {
            for (let byte in bytes.to_array()) {
                self.bytes.push(byte);
            }
            self.writes += 1;
        }

        function finish(self) -> int throws never { self.bytes.length() }
    }
}

function feed<T extends baml.Hash>(value: T) -> int[] throws never {
    let state = RecordingHasher { bytes: [], writes: 0 };
    value.hash(state);
    state.bytes
}

function digest<T extends baml.Hash>(value: T) -> int throws never {
    let state = baml.hash.DefaultHasher.new();
    value.hash(state);
    state.finish()
}

class Data<T> {
    first: T
    rest: T[]
}

class SameFields {
    first: int
    rest: int[]
}

enum Color {
    Red
    Blue
}

enum OtherColor {
    Red
    Blue
}

test "scalar equality produces identical bytes and hashes" {
    assert.equal(feed(0.0), feed(-0.0))
    assert.equal(feed(float.nan()), feed(-float.nan()))
    assert.equal(feed(float.nan()), feed(float.parse("NaN")))
    assert.equal(feed(123456789012345678901234567890n), feed(bigint.parse("123456789012345678901234567890")))
    assert.equal(feed(-123456789012345678901234567890n), feed(bigint.parse("-123456789012345678901234567890")))
    assert.equal(feed(uint8array.from_array([0, 1, 255])), feed(uint8array.from_array([0, 1, 255])))
    assert.equal(digest("same"), digest("same"))
    assert.equal(digest(null), digest(null))
    assert.equal(digest(true), digest(true))
}

test "type tags lengths and positions frame structural bytes" {
    assert.is_true(feed(1) != feed(1n))
    assert.is_true(feed(1) != feed("1"))
    assert.is_true(feed(["ab", "c"]) != feed(["a", "bc"]))
    assert.is_true(feed([1, 2]) != feed([2, 1]))
    assert.is_true(feed(Data<int> { first: 1, rest: [2] }) != feed(SameFields { first: 1, rest: [2] }))
    assert.is_true(feed(Color.Red) != feed(Color.Blue))
    assert.is_true(feed(Color.Red) != feed(OtherColor.Red))
}

test "map bytes ignore insertion order" {
    let a: map<string, int[]> = { "z": [3, 4], "a": [1, 2] };
    let b: map<string, int[]> = { "a": [1, 2], "z": [3, 4] };
    assert.equal(a, b)
    assert.equal(feed(a), feed(b))
    assert.equal(digest(a), digest(b))
}

class Key {
    id: int
    ignored: string

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.id == other.id }
    }

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            self.id.hash(state);
        }
    }
}

test "nested custom hash and user hasher suspend and resume" {
    let a = Key { id: 7, ignored: "left" };
    let b = Key { id: 7, ignored: "right" };
    assert.equal(feed(a), feed(b))
    assert.equal(feed(Data<Key> { first: a, rest: [a, b] }), feed(Data<Key> { first: b, rest: [b, a] }))
    let left: map<string, Key> = { "x": a };
    let right: map<string, Key> = { "x": b };
    assert.equal(feed(left), feed(right))
    assert.equal(digest(left), digest(right))
    let state = RecordingHasher { bytes: [], writes: 0 };
    [a, b].hash(state);
    assert.is_true(state.writes > 1)
    assert.equal(state.bytes, feed([a, b]))
}

class DefaultEq {
    value: int
    implements baml.ops.Equals {}
}

test "empty Equals adoption retains structural hash" {
    assert.equal(feed(DefaultEq { value: 3 }), feed(DefaultEq { value: 3 }))
}

class Node {
    value: int
    next: Node?
}

test "shared subtrees hash like duplicated equal subtrees" {
    let shared = Node { value: 2, next: null };
    let a = [shared, shared];
    let b = [Node { value: 2, next: null }, Node { value: 2, next: null }];
    assert.equal(a, b)
    assert.equal(feed(a), feed(b))
}

test "cyclic structural values panic" {
    let node = Node { value: 1, next: null };
    node.next = node;
    let caught = false;
    {
        feed(node);
    } catch (error) {
        baml.panics.UserPanic => { caught = true; }
    }
    assert.is_true(caught)
}

function membership(value: unknown) -> bool { value is baml.Hash }

class EqOnly {
    value: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { true }
    }
}

class OpaqueField {
    value: unknown
}

test "runtime Hash membership respects component eligibility and custom Equals" {
    assert.is_true(membership(1))
    assert.is_true(membership([1, 2]))
    assert.is_true(membership(Node { value: 1, next: null }))
    assert.is_true(membership(DefaultEq { value: 3 }))
    assert.is_true(membership(Key { id: 1, ignored: "x" }))
    assert.is_true(!membership(EqOnly { value: 1 }))
    assert.is_true(!membership([EqOnly { value: 1 }]))
    assert.is_true(!membership(OpaqueField { value: 1 }))
    assert.is_true(!membership((x: int) -> int throws never { x }))
}
