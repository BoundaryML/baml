// `baml.hash.DefaultHasher` keeps its state in Rust (`@@rust_type`). It is
// still an instance of its class: `match` and `==` treat it as one, it hashes
// the same whichever way it is reached, and it prints as its class.

function hasher_kind(v: unknown) -> string {
    match (v) {
        let h: baml.hash.DefaultHasher => "hasher",
        let s: string => "string",
        _ => "other",
    }
}

test "a DefaultHasher is an instance of its class" {
    let h = baml.hash.DefaultHasher.new();
    assert.equal(hasher_kind(h), "hasher")
    assert.equal(hasher_kind("x"), "string")
}

test "equal input hashes equally through fresh hashers" {
    let a = baml.hash.DefaultHasher.new();
    "hello".hash(a);
    let b = baml.hash.DefaultHasher.new();
    "hello".hash(b);
    assert.equal(a.finish(), b.finish())
    // Repeated calls without writes return the same value.
    assert.equal(a.finish(), a.finish())
}

test "hashers compare by identity" {
    let a = baml.hash.DefaultHasher.new();
    let b = baml.hash.DefaultHasher.new();
    assert.is_true(a == a)
    assert.is_true(a != b)
}

test "a hasher prints as its class" {
    assert.equal(`${baml.hash.DefaultHasher.new()}`, "DefaultHasher")
}

// A hasher's state is its own: a deep copy is a second hasher that starts
// where the original stood and goes on independently of it.

function hash_of_a() -> int {
    let h = baml.hash.DefaultHasher.new();
    "a".hash(h);
    h.finish()
}

function hash_of_a_then_b() -> int {
    let h = baml.hash.DefaultHasher.new();
    "a".hash(h);
    "b".hash(h);
    h.finish()
}

/// The original's digest after the copy was fed "b": still "a" alone.
function original_after_copy_written() -> int {
    let original = baml.hash.DefaultHasher.new();
    "a".hash(original);
    let copy = baml.deep_copy(original);
    "b".hash(copy);
    original.finish()
}

/// The copy's digest after it was fed "b": "a" then "b".
function copy_after_written() -> int {
    let original = baml.hash.DefaultHasher.new();
    "a".hash(original);
    let copy = baml.deep_copy(original);
    "b".hash(copy);
    copy.finish()
}

test "a deep copy of a hasher is independent of the original" {
    assert.equal(original_after_copy_written(), hash_of_a());
    assert.equal(copy_after_written(), hash_of_a_then_b());
    // Negative control: the two digests differ, so the first assertion
    // could fail.
    assert.is_true(hash_of_a() != hash_of_a_then_b())
}
