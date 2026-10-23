//! Mutual exclusion between tasks.

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
