// A class that holds `$rust_type` state is created only by its own functions.
// A class literal could only copy a handle out of another value, which would
// relabel what the handle means, so the literal itself is refused.

// A handle moved across classes would hand a limit's functions a cancel token.
function cross_class_handle() -> int throws never {
    let token = baml.spawn.CancelToken.new();
    let fake = baml.sync.Semaphore { _handle: token._handle };
    fake.capacity()
}

// A spread is the same literal.
function spread_copy(limit: baml.sync.Semaphore) -> baml.sync.Semaphore throws never {
    baml.sync.Semaphore { ...limit }
}
