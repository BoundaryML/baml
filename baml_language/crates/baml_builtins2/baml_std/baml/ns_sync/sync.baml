//! Mutual exclusion between tasks that share an object.

/// Runs `fn` while holding `key`, and returns what it returns.
///
/// Only one task at a time runs an `atomic` block on a given key; others
/// calling `atomic` with the same key wait their turn, in the order they
/// arrived. Blocks on different keys never wait for each other. A task already
/// inside an `atomic` block on a key (one nested in another, however many calls
/// apart) enters again without waiting, and the key is released when the
/// outermost block exits. A spawned task is a different task: it does not share
/// its spawner's hold.
///
/// Only other `atomic` blocks on the same key are excluded; code that reads or
/// writes the object outside such a block is not protected. Objects reachable
/// from `key` are not locked with it.
///
/// A key is a class instance, compared by identity: every reference to one
/// object is the same key, and two objects with equal fields are different
/// keys. A value that passes through a host SDK comes back as a copy, which is
/// a new object and so a new key.
///
/// A list holds every instance in it together. The block waits, holding none
/// of them, until it can take all of them at once, so blocks naming the same
/// objects in different orders take turns instead of deadlocking. Instances
/// the task already holds are re-entered at once and stay held while it waits
/// for the rest. The list is copied on entry: changing it inside `fn` does not
/// change what is held.
///
/// `fn` runs once, in the calling task. The key is released however `fn` exits:
/// returning, throwing, panicking or being cancelled. Writes `fn` made before
/// exiting are kept, not rolled back. Waiting for the key is a cancellation
/// checkpoint; taking a free key, or re-entering a held one, is not. The key
/// stays held while `fn` awaits or does I/O.
///
/// # Parameters
///
/// - `key`: the object to hold, or a list of objects to hold together.
///   A typed list like `Account[]` passes as
///   `accounts.map((a) -> reflect.AnyClass { a })`.
/// - `fn`: what to run while holding it.
///
/// # Returns
///
/// What `fn` returns.
///
/// # Throws
///
/// Whatever `fn` throws, after releasing the key.
///
/// # Examples
///
/// ```baml
/// class Account {
///     balance: int,
///
///     function deposit(self, amount: int) -> void {
///         baml.sync.atomic(self, () -> void {
///             self.balance += amount;
///         })
///     }
/// }
///
/// // Two transfers in opposite directions take turns.
/// function transfer(from: Account, to: Account, amount: int) -> bool {
///     baml.sync.atomic([from, to], () -> bool {
///         if (from.balance < amount) {
///             return false;
///         }
///         from.deposit(-amount);   // this task holds `from`: enters at once
///         to.deposit(amount);
///         true
///     })
/// }
/// ```
function atomic<R, E>(
    key: reflect.AnyClass | reflect.AnyClass[],
    fn: () -> R throws E,
) -> R throws E {
    let held = _held(key);
    let ticket = _enter(held);
    // Armed before the wait, so a wait that ends in cancellation still runs
    // it; `_exit` releases only keys this task holds.
    defer { _exit(held) }
    if (ticket != null) {
        _wait(ticket);
    }
    fn()
}

/// (internal) `key` itself, or a copy of a list key: what the block holds,
/// whatever `fn` does to the list.
//baml:mut_vm
//baml:fallible
function _held<K>(key: K) -> K throws never {
    $rust_function
}

/// (internal) Takes `key`'s objects, or re-enters those this task holds;
/// returns null. When another task holds any of the rest, queues this one and
/// returns its place in line.
//baml:mut_vm
//baml:fallible
function _enter(key: reflect.AnyClass | reflect.AnyClass[]) -> $rust_type? throws never {
    $rust_function
}

/// (internal) Waits until the keys queued for by `_enter` are this task's.
function _wait(ticket: $rust_type) -> null throws never {
    $rust_io_function
}

/// (internal) Undoes one `_enter` of `key` by this task.
//baml:mut_vm
//baml:fallible
function _exit(key: reflect.AnyClass | reflect.AnyClass[]) -> void throws never {
    $rust_function
}
