//! Synchronization between tasks: `Lock` for mutual exclusion, `Semaphore`
//! to cap how many holders run at once.

/// A lock that one task holds at a time.
///
/// A task that holds the lock can take it again without waiting (an `acquire`
/// or `with` nested in another, however many calls apart); the lock is free
/// again once every one of those holds is released. Other tasks wait their
/// turn, in the order they arrived. A spawned task is a different task: it does
/// not share its spawner's hold.
///
/// The lock protects nothing by itself: only tasks that take the same lock
/// exclude each other. Share one lock wherever code needs it, for example as a
/// field of the object it guards.
///
/// The lock stays held while its holder awaits or does I/O. A lock its task
/// still holds when the task ends is released then.
///
/// # Examples
///
/// ```baml
/// class Counter {
///     n: int,
///     lock: baml.sync.Lock,
///
///     function increment(self) -> int {
///         self.lock.with(() -> int {
///             self.n += 1;
///             self.n
///         })
///     }
/// }
/// ```
class Lock {
    _handle: $rust_type,

    /// Creates a lock no task holds.
    //baml:mut_vm
    function new() -> Lock throws never {
        $rust_function
    }

    /// Waits until this task holds the lock, and returns the guard that
    /// releases this hold.
    ///
    /// Release it with `defer` right away, so every exit releases it:
    ///
    /// ```baml
    /// let guard = lock.acquire();
    /// defer { guard.release() }
    /// ```
    ///
    /// Waiting is a cancellation checkpoint; taking a free lock, or one this
    /// task already holds, is not. A task cancelled while waiting never holds
    /// the lock.
    function acquire(self) -> Guard throws never {
        let ticket = self._enter();
        let acquired = false;
        // Armed before the wait, so a wait that ends in cancellation gives
        // back a grant that raced it, and leaves the queue.
        defer {
            if (!acquired) {
                self._exit()
            }
        }
        if (ticket != null) {
            _wait(ticket);
        }
        let guard = self._guard();
        acquired = true;
        guard
    }

    /// Runs `fn` while holding the lock, and returns what it returns.
    ///
    /// The lock is released however `fn` exits: returning, throwing, panicking
    /// or being cancelled. Changes `fn` made before exiting are kept.
    ///
    /// # Throws
    ///
    /// Whatever `fn` throws, after releasing the lock.
    function with<R, E>(self, fn: () -> R throws E) -> R throws E {
        let ticket = self._enter();
        // Armed before the wait, like `acquire`'s; `_exit` releases only a
        // hold this task has.
        defer { self._exit() }
        if (ticket != null) {
            _wait(ticket);
        }
        fn()
    }

    /// (internal) Takes the lock, or enters it again, and returns null. When
    /// another task holds it, queues this one and returns its place in line.
    //baml:mut_vm
    function _enter(self) -> $rust_type? throws never {
        $rust_function
    }

    /// (internal) Undoes one `_enter` by this task.
    //baml:mut_vm
    function _exit(self) -> void throws never {
        $rust_function
    }

    /// (internal) A guard for this task's current hold.
    //baml:mut_vm
    function _guard(self) -> Guard throws never {
        $rust_function
    }
}

/// One hold of a [`Lock`], from `Lock.acquire`.
class Guard {
    _handle: $rust_type,

    /// Releases this hold. Releasing it again does nothing.
    ///
    /// # Panics
    ///
    /// - `baml.panics.LockNotHeld` if called from a task other than the one
    ///   that acquired it, while that task still holds the lock.
    //baml:mut_vm
    //baml:fallible
    function release(self) -> void throws never {
        $rust_function
    }
}

/// (internal) Waits until the lock queued for by `Lock._enter` is this task's.
function _wait(ticket: $rust_type) -> null throws never {
    $rust_io_function
}

/// A pool of `capacity` slots: at most that many holders run at once.
///
/// A slot is taken by a task spawned `with` the semaphore, for the whole task,
/// or by `acquire` or `with` inside a running task. Holders over the cap wait
/// in line, in the order they arrived; a waiting holder can be cancelled.
///
/// Slots have no owner: a task holding one does not get another for free (a
/// task spawned `with` a semaphore that calls its `with` takes a second slot,
/// so at capacity 1 it waits forever), and a permit can be released by any
/// task. A spawn with several semaphores starts once every one has a free slot,
/// and holds none of them while it waits; `with` calls nested in each other
/// take theirs one at a time.
///
/// # Examples
///
/// ```baml
/// let provider = baml.sync.Semaphore.new(8);
/// let futures = ids.map((id) -> { spawn with provider { fetch(id) } });
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
