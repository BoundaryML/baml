// Fixtures for the error-return contract (31b-phase1 / 31h-phase7).
//
// The host cannot call stdlib *native* builtins (`baml.json.parse`,
// `baml.sys.panic`, `baml.sys.exit`, `baml.sys.sleep`) directly as top-level
// entry points — native functions aren't invokable as entry points (the
// engine returns `NotInvokableAsEntry`). So we wrap each in a *bytecode*
// function the host can call, which then invokes the native internally; the
// thrown error / panic / exit propagates out through the wrapper.

class MyError {
  code int
  detail string
}

// User-throw arm: `e.value` must be the declared `MyError` instance.
function ThrowMyError() -> int throws MyError {
  throw MyError { code: 42, detail: "boom" }
}

// Node maps BAML `int` to JavaScript `number`, whose exact integer range ends
// at 2^53 - 1. These two fixtures ensure a representation failure nested in a
// thrown value is surfaced explicitly rather than silently dropping `.value`.
function ThrowUnsafeIntError() -> int throws MyError {
  throw MyError { code: 9007199254740993, detail: "unsafe integer" }
}

function PanicWithUnsafeInt() -> int {
  9007199254740993 / 0
}

// stdlib error arm: bad input throws `baml.json.JsonParseError`.
function ParseJson(s: string) -> baml.json.json throws baml.json.JsonParseError {
  baml.json.parse(s)
}

// user-initiated panic arm: `baml.sys.panic` throws `baml.panics.UserPanic`.
function DoPanic(message: string) -> never {
  baml.sys.panic(message)
}

// clean-exit arm: `baml.sys.exit(code)` must terminate the process with `code`.
function DoExit(code: int) -> never {
  baml.sys.exit(code)
}

// cancellation arm: a sleep long enough to abort mid-flight.
function SleepMs(ms: int) -> null throws baml.errors.Io {
  baml.sys.sleep(baml.time.Duration.from_nanoseconds(ms * 1000000n))
}
