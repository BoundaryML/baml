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
