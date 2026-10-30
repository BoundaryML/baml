// A class that holds `$rust_type` state is created only by its own functions.
// A class literal could only copy a handle out of another value, which would
// relabel what the handle means, so the literal itself is refused.

// A handle moved across classes would hand a glob's functions a generator's
// state.
function cross_class_handle() -> bool throws never {
    let rng = baml.random.Xoshiro256PlusPlus.new(seed = "0123456789abcdef0123456789abcdef".to_utf8());
    let fake = baml.glob.Glob { _handle: rng._state };
    fake.matches("x")
}

// A spread is the same literal.
function spread_copy(limit: baml.spawn.Limit) -> baml.spawn.Limit throws never {
    baml.spawn.Limit { ...limit }
}

// A `@@rust_type` class is the same rule with no field to copy: only its own
// functions create one.
function tagged_literal() -> baml.hash.DefaultHasher throws never {
    baml.hash.DefaultHasher {}
}
