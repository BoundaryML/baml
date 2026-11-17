//! Synchronization between tasks: `atomic` to guard an object's own state,
//! `Semaphore` to cap how many holders run at once.

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

/// A pool of `capacity` slots: at most that many holders run at once.
///
/// A slot is taken by a task spawned `with` the semaphore, for the whole task,
/// or by `acquire` or `with` inside a running task. Holders over the cap wait
/// in line, in the order they arrived; a waiting holder can be cancelled.
///
/// Slots have no owner, so a holder never re-enters: every `acquire` or `with`
/// takes a slot of its own, and any task may release a permit. A `with` nested
/// inside another on the same semaphore (in one task, or in a task spawned
/// `with` it) takes a second slot, so once outer blocks hold every slot, every
/// inner one waits forever. A spawn with several semaphores starts once every
/// one has a free slot, and holds none of them while it waits; `with` calls
/// nested in each other take theirs one at a time.
///
/// # Examples
///
/// ```baml
/// function fetch_all(urls: string[]) -> string[] {
///     let provider = baml.sync.Semaphore.new(8);   // at most 8 fetches at once
///     await baml.future.all(urls.map((url) -> {
///         spawn with provider { baml.http.fetch(url).text() }
///     }))
/// }
/// ```
class Semaphore {
    _handle: $rust_type,

    /// Creates a semaphore with `capacity` free slots.
    ///
    /// # Throws
    ///
    /// `InvalidArgument` if `capacity` is not positive.
    //baml:mut_vm
    function new(capacity: int) -> Semaphore throws root.errors.InvalidArgument {
        $rust_function
    }

    /// How many slots this semaphore has.
    //baml:vm
    function capacity(self) -> int throws never {
        $rust_function
    }

    /// How many slots are held right now.
    //baml:vm
    function active_count(self) -> int throws never {
        $rust_function
    }

    /// How many holders are waiting for a slot right now.
    //baml:vm
    function queued_count(self) -> int throws never {
        $rust_function
    }

    /// Waits for a free slot, and returns the permit that releases it.
    ///
    /// Release it with `defer` right away, so every exit releases it:
    ///
    /// ```baml
    /// let permit = semaphore.acquire();
    /// defer { permit.release() }
    /// ```
    ///
    /// Waiting is a cancellation checkpoint; taking a free slot is not. A task
    /// cancelled while waiting holds no slot.
    function acquire(self) -> Permit throws never {
        let held = self._admit();
        let acquired = false;
        // Armed before the wait, so a wait that ends in cancellation leaves
        // the queue, or gives back a slot that raced it.
        defer {
            if (!acquired) {
                _release_permit(held)
            }
        }
        if (!_admitted(held)) {
            _wait_permit(held);
        }
        let permit = _permit(held);
        acquired = true;
        permit
    }

    /// Runs `fn` while holding a slot, and returns what it returns.
    ///
    /// The slot is released however `fn` exits: returning, throwing, panicking
    /// or being cancelled.
    ///
    /// # Throws
    ///
    /// Whatever `fn` throws, after releasing the slot.
    function with<R, E>(self, fn: () -> R throws E) -> R throws E {
        let held = self._admit();
        defer { _release_permit(held) }
        if (!_admitted(held)) {
            _wait_permit(held);
        }
        fn()
    }

    /// (internal) Takes a free slot, or queues for one, and returns the hold.
    //baml:mut_vm
    function _admit(self) -> $rust_type throws never {
        $rust_function
    }
}

/// One slot of a [`Semaphore`], from `Semaphore.acquire`.
///
/// Any task may release it. A permit nobody can release any more (dropped
/// without `release`) gives its slot back when it is garbage collected, which
/// can be much later.
class Permit {
    _handle: $rust_type,

    /// Releases this slot. Releasing it again does nothing.
    //baml:vm
    function release(self) -> void throws never {
        $rust_function
    }
}

implements<T, E> root.spawn.Modifier<T, E> for Semaphore {
    function apply(self, plan: root.spawn.Plan<T, E>) -> root.spawn.Plan<T, E> throws never {
        root.spawn.Plan { ...plan, semaphores: plan.semaphores.concat([self]) }
    }
}

/// (internal) Whether the hold from `Semaphore._admit` has its slot.
//baml:vm
function _admitted(held: $rust_type) -> bool throws never {
    $rust_function
}

/// (internal) Waits until the hold from `Semaphore._admit` has its slot.
function _wait_permit(held: $rust_type) -> null throws never {
    $rust_io_function
}

/// (internal) A permit for the hold from `Semaphore._admit`.
//baml:mut_vm
function _permit(held: $rust_type) -> Permit throws never {
    $rust_function
}

/// (internal) Releases the hold from `Semaphore._admit`, or withdraws it from
/// the queue.
//baml:vm
function _release_permit(held: $rust_type) -> void throws never {
    $rust_function
}
