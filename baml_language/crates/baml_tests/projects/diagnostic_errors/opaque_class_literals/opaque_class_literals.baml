// A class that holds `$rust_type` state is created only by its own functions.
// A class literal could only copy a handle out of another value, which would
// relabel what the handle means, so the literal itself is refused.

// A handle moved across classes would hand a lock's functions an output
// format's schema.
function cross_class_handle() -> baml.fs.Lock {
    let format = ai.internal.build_output_format(string.type());
    baml.fs.Lock { _handle: format._data }
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
