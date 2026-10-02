/// A value that is implicitly visible to everything a call does, including every
/// `spawn`ed thread started from it, without being passed as a parameter.
///
/// Modelled on Node.js's `AsyncLocalStorage`. Each thread carries a context: a
/// set of values, one per storage `name`. `run` sets this storage's value for
/// the duration of a body, and a `spawn` starts its thread with a snapshot of
/// the context at the moment it was spawned, so values flow from parent to
/// child. A child that changes its own context never affects its parent or its
/// siblings.
///
/// A storage is identified by its `name`, not by the object: two storages with
/// the same name read and write the same value. That is what lets a host SDK
/// seed a storage for a whole call, by passing values keyed by name alongside
/// the call's arguments.
///
/// # Examples
///
/// ```baml
/// let request_id = baml.SpawnLocalStorage.new("request_id", "none");
///
/// request_id.run("req-1", () -> string {
///     let child = spawn { request_id.get() };
///     await child // "req-1"
/// });
/// request_id.get() // "none"
/// ```
class SpawnLocalStorage<T> {
    /// The key this storage's value is kept under in the context.
    name: string,
    /// What `get` returns when the context holds no value for `name`.
    default: T,

    /// Create a storage for `name` that reads `default` until a value is set.
    ///
    /// # Parameters
    ///
    /// - `name`: the key the value is kept under. Storages that share a name
    ///   share a value, and a host SDK seeds a storage by this name.
    /// - `default`: returned by `get` when no value is set for `name`.
    function new(name: string, default: T) -> SpawnLocalStorage<T> throws never {
        SpawnLocalStorage { name: name, default: default }
    }

    /// The current value of this storage.
    ///
    /// # Returns
    ///
    /// The value set by the innermost enclosing `run` (or `enter_with`) on this
    /// thread or, failing that, the value inherited from the thread that spawned
    /// it or from the host call that started it. Returns `default` when no value
    /// is set.
    ///
    /// A value a host supplies arrives as JSON and is decoded into `T` the way
    /// `baml.json.to` does, on every call. When the value cannot be read as a
    /// `T` (a host passed a mismatched value, or two storages of different types
    /// share a name) this also returns `default`.
    function get(self) -> T throws never {
        let frame = baml._spawn_local_frame();
        if (!frame.has(self.name)) {
            return self.default;
        }
        match (frame[self.name]) {
            let value: T => value,
            // A host-supplied value arrives as `json`; decode it into `T` (an
            // enum from its variant name, a `bigint` from a numeric string, a
            // class from a map, ...). The arms name json's members because a
            // pattern of the `baml.json.json` alias itself does not match an
            // `unknown` scrutinee.
            let value: map<string, baml.json.json> => self._decode(value),
            let value: baml.json.json[] => self._decode(value),
            let value: string => self._decode(value),
            let value: int => self._decode(value),
            let value: float => self._decode(value),
            let value: bigint => self._decode(value),
            let value: bool => self._decode(value),
            _ => self.default,
        }
    }

    /// (internal) Decode a host-supplied value, or `default` if it is not a `T`.
    function _decode(self, value: baml.json.json) -> T throws never {
        baml.json.to<T>(value) catch (_) {
            baml.json.DecodeError => self.default,
        }
    }

    /// Run `body` with this storage set to `value`, then restore the previous
    /// value.
    ///
    /// Every `spawn` started while `body` runs inherits `value`, and keeps it
    /// even after `run` returns. The previous value is restored however `body`
    /// finishes, whether it returns, throws or panics.
    ///
    /// # Parameters
    ///
    /// - `value`: what `get` returns while `body` runs.
    /// - `body`: the code to run.
    ///
    /// # Returns
    ///
    /// Whatever `body` returns.
    ///
    /// # Throws
    ///
    /// Whatever `body` throws.
    function run<R, E>(self, value: T, body: () -> R throws E) -> R throws E {
        let prior = baml._spawn_local_frame();
        let next = baml._spawn_local_frame_copy(prior);
        next[self.name] = value;
        baml._set_spawn_local_frame(next);
        defer {
            baml._set_spawn_local_frame(prior);
        }
        body()
    }

    /// Run `body` with this storage unset, so that `get` returns `default`, then
    /// restore the previous value.
    ///
    /// Every `spawn` started while `body` runs inherits the unset storage. Other
    /// storages are unaffected.
    ///
    /// # Parameters
    ///
    /// - `body`: the code to run.
    ///
    /// # Returns
    ///
    /// Whatever `body` returns.
    ///
    /// # Throws
    ///
    /// Whatever `body` throws.
    function exit<R, E>(self, body: () -> R throws E) -> R throws E {
        let prior = baml._spawn_local_frame();
        let next = baml._spawn_local_frame_copy(prior);
        next.delete(self.name);
        baml._set_spawn_local_frame(next);
        defer {
            baml._set_spawn_local_frame(prior);
        }
        body()
    }

    /// Set this storage to `value` for the rest of the current thread.
    ///
    /// Unlike `run`, nothing restores the previous value: it stays set after the
    /// calling function returns, until the enclosing `run` or `exit` (if any)
    /// finishes. Prefer `run`.
    ///
    /// # Parameters
    ///
    /// - `value`: what `get` returns from now on.
    function enter_with(self, value: T) -> void throws never {
        let next = baml._spawn_local_frame_copy(baml._spawn_local_frame());
        next[self.name] = value;
        baml._set_spawn_local_frame(next);
    }

    /// Unset this storage for the rest of the current thread, so that `get`
    /// returns `default`.
    ///
    /// Like `enter_with`, nothing restores the previous value until the enclosing
    /// `run` or `exit` (if any) finishes. Threads already spawned keep the value
    /// they inherited.
    function disable(self) -> void throws never {
        let next = baml._spawn_local_frame_copy(baml._spawn_local_frame());
        next.delete(self.name);
        baml._set_spawn_local_frame(next);
    }
}

// The context is a `map<string, unknown>` that is never mutated once it has been
// installed: parent and child threads share it by reference, so every change
// installs a fresh copy instead.

/// (internal) The current thread's spawn-local context, keyed by storage name.
function _spawn_local_frame() -> map<string, unknown> throws never {
    match (baml._spawn_local_frame_raw()) {
        let frame: map<string, unknown> => frame,
        _ => {
            let empty: map<string, unknown> = {};
            empty
        },
    }
}

/// (internal) A copy of `frame` that can be changed and then installed.
function _spawn_local_frame_copy(frame: map<string, unknown>) -> map<string, unknown> throws never {
    let copy: map<string, unknown> = {};
    for (let key in frame.keys()) {
        copy[key] = frame[key];
    }
    copy
}

/// (internal) The current thread's context value, `null` when none is installed.
//baml:vm
function _spawn_local_frame_raw() -> unknown throws never {
    $rust_function
}

/// (internal) Install `frame` as the current thread's context. Threads spawned
/// from now on start with it.
function _set_spawn_local_frame(frame: map<string, unknown>) -> void throws never {
    baml._set_spawn_local_frame_raw(frame);
}

/// (internal) Install `frame`, which must be a `map<string, unknown>`, as the
/// current thread's context.
//baml:mut_vm
function _set_spawn_local_frame_raw(frame: unknown) -> void throws never {
    $rust_function
}
