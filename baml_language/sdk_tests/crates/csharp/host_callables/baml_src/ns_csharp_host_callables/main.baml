function invoke_optionals(
    callback: (x: int, y?: int, z?: string) -> string,
    x: int,
) -> string[] {
    [
        callback(x),
        callback(x, z = "tail"),
        callback(x, y = 2, z = "both"),
    ]
}

function invoke_deferred(callback: (x: int) -> int, x: int) -> int {
    callback(x) + 1
}

function propagate_host_throw(
    callback: (x: int) -> string throws baml.errors.HostCallable,
    x: int,
) -> string throws baml.errors.HostCallable {
    callback(x)
}

function invoke_cancelable(callback: () -> string) -> string {
    callback()
}

function apply<T, R>(callback: (T) -> R, value: T) -> R {
    callback(value)
}

function invoke_generic_optionals<T, R>(
    callback: (value: T, fallback?: T) -> R,
    value: T,
    fallback: T,
) -> R[] {
    [
        callback(value),
        callback(value, fallback = fallback),
    ]
}

function visit<T>(callback: (T) -> void, value: T) -> int {
    callback(value)
    1
}

function produce<R>(callback: () -> R) -> R {
    callback()
}

function invoke_zero_void(callback: () -> void) -> int {
    callback()
    1
}

// A unit callback takes its arguments positionally like any other, whatever
// its arity and whichever of `void` and `null` spells its return type.
function call_void_callback_two_args(callback: (int, string) -> void, value: int, label: string) -> int {
    callback(value, label)
    value
}

function call_void_callback_three_args(
    callback: (int, string, bool) -> null,
    value: int,
    label: string,
    flag: bool,
) -> int {
    callback(value, label, flag)
    value
}

class CallbackBox<T> {
    value T

    function transform<R>(self, callback: (T) -> R) -> R {
        callback(self.value)
    }
}

class CallbackHost {
    function apply<T, R>(callback: (T) -> R, value: T) -> R {
        callback(value)
    }
}

function ping() -> int {
    42
}

class ReturnedPerson {
    name string
    age int
}

function make_adder(offset: int) -> (value: int) -> int throws never {
    return (value: int) -> int { offset + value }
}

function make_pair_builder(base: int) -> (delta: int, label: string) -> ReturnedPerson throws never {
    return (delta: int, label: string) -> ReturnedPerson {
        ReturnedPerson {
            name: label,
            age: base + delta,
        }
    }
}

function make_counter(start: int) -> () -> int throws never {
    let current = start
    return () -> int {
        current += 1
        current
    }
}

// A returned closure with no value: the host's call completes with nothing to
// read back.
function make_void_callback() -> () -> void throws never {
    return () -> void {}
}

// The retained host callback is the only observer of the returned closure's
// arguments.
//
// BUG: the callback's parameters are named only to match the returned
// closure's. The C# generator names a type's helpers by its rendering, which
// leaves required parameter names out, yet emits one set of helpers per
// distinct type: two callable types differing only in those names come out
// as duplicate definitions, and the generated program does not compile.
function make_void_forwarder(
    callback: (value: int, label: string) -> void throws baml.errors.HostCallable,
) -> (value: int, label: string) -> void throws baml.errors.HostCallable {
    return (value: int, label: string) -> void throws baml.errors.HostCallable {
        callback(value, label)
    }
}
