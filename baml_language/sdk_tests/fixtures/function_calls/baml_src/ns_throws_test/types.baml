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

// Multi-step cancellation arm: marks entry, sleeps long enough to abort
// mid-flight, then performs a *second* observable step. A cancel delivered
// during the sleep must prevent `later_path` from ever appearing — proving the
// runtime stops walking the function rather than merely returning early to the
// host. Marker files are the only channel the host can observe mid-call.
function SleepThenMarkMs(ms: int, entry_path: string, later_path: string) -> null throws baml.errors.Io {
  baml.fs.write(entry_path, "entered");
  baml.sys.sleep(baml.time.Duration.from_nanoseconds(ms * 1000000n));
  baml.fs.write(later_path, "later")
}
