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
