// Test project for event system observability
// This project tests:
// 1. Function start/end events
// 2. Nested function calls and event ordering
// 3. (Future) Custom events via baml.events.send()
// 4. (Future) Structured logging via log.info()

function simple_function() -> int {
    42
}

function nested_outer() -> int {
    nested_inner() + 1
}

function nested_inner() -> int {
    10
}

function with_args(a: int, b: string) -> string {
    b + ": " + baml.to_string(a)
}

function with_complex_return() -> map<string, int> {
    { "one": 1, "two": 2, "three": 3 }
}

// Function that will later use baml.events.send()
function future_custom_events(name: string) -> null {
    // TODO: baml.events.send("user_action", { name: name })
    null
}

// Function that will later use log.info()
function future_logging(message: string) -> null {
    // TODO: log.info(message, { source: "event_system_test" })
    null
}
