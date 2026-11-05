// A class that holds `$rust_type` state is created only by its own functions.
// A class literal could only copy a handle out of another value, which would
// relabel what the handle means, so the literal itself is refused.

// A handle copied into a second lock would let two locks release one.
function copied_handle(lock: baml.fs.Lock) -> baml.fs.Lock {
    baml.fs.Lock { _handle: lock._handle }
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
