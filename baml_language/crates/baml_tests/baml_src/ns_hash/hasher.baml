class RecordingHasher {
    bytes uint8array
    writes int

    implements baml.hash.Hasher {
        function write(self, bytes: uint8array) -> void throws never {
            self.bytes = self.bytes.concat(bytes);
            self.writes += 1;
        }

        function finish(self) -> int throws never {
            self.bytes.length()
        }
    }
}

class ManualHash {
    value string

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            self.value.hash(state)
        }
    }
}

function new_recording_hasher() -> RecordingHasher throws never {
    RecordingHasher { bytes: b"", writes: 0 }
}

function feed<T extends baml.Hash>(value: T, state: baml.hash.Hasher) -> void throws never {
    value.hash(state)
}

function write_through_interface(state: baml.hash.Hasher, bytes: uint8array) -> int throws never {
    state.write(bytes)
    state.finish()
}

test "default hasher streams bytes independently of chunking" {
    let whole = baml.hash.DefaultHasher.new();
    whole.write("abcdef".to_utf8())
    let chunks = baml.hash.DefaultHasher.new();
    chunks.write("ab".to_utf8())
    chunks.write("".to_utf8())
    chunks.write("cdef".to_utf8())
    assert.equal(chunks.finish(), whole.finish())
}

test "finish is repeatable and does not reset" {
    let state = baml.hash.DefaultHasher.new();
    state.write("abc".to_utf8())
    let first = state.finish();
    assert.equal(state.finish(), first)
    state.write("def".to_utf8())
    let expected = baml.hash.DefaultHasher.new();
    expected.write("abcdef".to_utf8())
    assert.equal(state.finish(), expected.finish())
}

test "hasher references share the same state" {
    let state = baml.hash.DefaultHasher.new();
    let alias: baml.hash.Hasher = state;
    alias.write("shared".to_utf8())
    let expected = baml.hash.DefaultHasher.new();
    expected.write("shared".to_utf8())
    assert.equal(state.finish(), expected.finish())
}

test "users can implement Hasher and use its methods as values" {
    let state = new_recording_hasher();
    assert.equal(write_through_interface(state, "abc".to_utf8()), 3)
    let view: baml.hash.Hasher = state;
    let write = view.write;
    let finish = view.finish;
    write("de".to_utf8())
    assert.equal(finish(), 5)
    assert.equal(state.bytes, "abcde".to_utf8())
}

test "custom Hash feeds the caller's Hasher without extra encoding" {
    let state = new_recording_hasher();
    feed(ManualHash { value: "hello" }, state)
    let expected = new_recording_hasher();
    "hello".hash(expected)
    assert.equal(state.bytes, expected.bytes)
    assert.is_true(state.writes > 0)
    assert.equal(state.finish(), state.bytes.length())
}

interface HashableItem requires baml.Hash {}

interface HashableChild requires HashableItem {}

class HashableNumber {
    value int

    implements HashableItem {}
    implements HashableChild {}
}

class AbstractHashField {
    item HashableChild
}

test "Hash requirements remain visible through abstract fields" {
    let first = baml.hash.DefaultHasher.new();
    let second = baml.hash.DefaultHasher.new();
    AbstractHashField { item: HashableNumber { value: 17 } }.hash(first)
    AbstractHashField { item: HashableNumber { value: 17 } }.hash(second)
    assert.equal(first.finish(), second.finish())
}

class AllocatingHasher {
    delegate baml.hash.DefaultHasher
    scratch int[]

    implements baml.hash.Hasher {
        function write(self, bytes: uint8array) -> void throws never {
            let index = 0;
            while (index < 4096) {
                self.scratch = [index, index + 1];
                index += 1;
            }
            self.delegate.write(bytes)
        }

        function finish(self) -> int throws never {
            self.delegate.finish()
        }
    }
}

test "structural hashing retains pending values across allocation callbacks" {
    let values = [
        ManualHash { value: "first" },
        ManualHash { value: "second" },
        ManualHash { value: "third" },
    ];
    let expected = baml.hash.DefaultHasher.new();
    values.hash(expected)
    let allocating = AllocatingHasher {
        delegate: baml.hash.DefaultHasher.new(),
        scratch: [],
    };
    values.hash(allocating)
    assert.equal(allocating.finish(), expected.finish())
    assert.equal(allocating.scratch, [4095, 4096])
}

class FailingHasher {
    implements baml.hash.Hasher {
        function write(self, bytes: uint8array) -> void throws never {
            assert.is_true(false)
        }

        function finish(self) -> int throws never {
            0
        }
    }
}

test "hasher callback panics propagate and leave the caller usable" {
    let caught = {
        [1, 2].hash(FailingHasher {});
        false
    } catch (error) {
        let error: baml.panics.AssertionFailed => true
    };
    assert.is_true(caught)
    let state = baml.hash.DefaultHasher.new();
    1.hash(state)
    let expected = baml.hash.DefaultHasher.new();
    1.hash(expected)
    assert.equal(state.finish(), expected.finish())
}

class OptionalHash {
    value int

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher, salt: int = 7) -> void throws never {
            (self.value + salt).hash(state)
        }
    }
}

class OptionalWriter {
    delegate baml.hash.DefaultHasher
    observed int

    implements baml.hash.Hasher {
        function write(self, bytes: uint8array, marker: int = 17) -> void throws never {
            self.observed = marker;
            self.delegate.write(bytes)
        }

        function finish(self) -> int throws never {
            self.delegate.finish()
        }
    }
}

test "Hash and Hasher callbacks preserve optional implementation parameters" {
    let state = OptionalWriter { delegate: baml.hash.DefaultHasher.new(), observed: 0 };
    feed(OptionalHash { value: 1 }, state)
    let expected = baml.hash.DefaultHasher.new();
    8.hash(expected)
    assert.equal(state.finish(), expected.finish())
    assert.equal(state.observed, 17)
}
