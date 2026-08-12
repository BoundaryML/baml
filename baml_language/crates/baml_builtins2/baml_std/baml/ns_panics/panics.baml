/// Raised when an integer is divided by zero.
class DivisionByZero {
    dividend: int,
}

/// Raised when an array or string is accessed with an out-of-bounds index.
class IndexOutOfBounds {
    index: int,
    length: int,
}

/// Raised when VM field access targets a stale or invalid field slot.
class InvalidFieldAccess {
    field_index: int,
    field_count: int,
}

/// Raised when a map is accessed with a key that does not exist.
class MapKeyNotFound {
    key: string,
}

/// Raised when the call stack depth limit is exceeded.
class StackOverflow {
    message: string,
}

/// Raised when an `assert` statement fails.
class AssertionFailed {
    message: string,
}

/// Raised when execution reaches a branch that should be impossible.
class Unreachable {
    message: string,
}

/// A cancellation was requested.
class Cancelled {
    message: string,
}

/// A user-caused panic from `baml.sys.panic`.
class UserPanic {
    message: string,
}

/// A clean process-termination request from `baml.sys.exit(code)`.
///
/// Catchable like any other panic; if left unhandled, the engine
/// terminates the process with this code. Patterned after Python's
/// `SystemExit`.
class Exit {
    code: int,
}

/// Memory allocation failure. This happens when an operation would have caused an unrecoverable
/// Out-Of-Memory error so we panic instead. Note that not all memory allocation failures are
/// guaranteed to panic; some may cause a hard failure.
class AllocFailure {
    message: string,
}

/// A required host resource is unavailable — e.g. the system entropy source
/// (`getrandom`) is not accessible in a sandboxed/isolated environment.
/// `resource` identifies which subsystem is unavailable (`"entropy"`, etc.).
class HostUnavailable {
    resource: string,
    message: string,
}

/// The right operand of a bigint shift (`<<` / `>>`) was negative.
/// The type system can't rule this out because the shift count is a
/// runtime `bigint` value.
class NegativeBitShift {
    message: string,
}

/// An internal BAML engine failure surfaced to the host. Opaque: the specific
/// engine error (an `EngineError` variant) is carried as text in `message`,
/// since these are engine/VM-internal failures with nothing host-actionable to
/// branch on. Synthesized host-side from any `EngineError`.
class SdkPanic {
    message: string,
}

/// A host callable violated its typed contract: it returned a value of the
/// wrong type, or threw a value that does not match its declared error
/// contract. Catchable like any other panic.
class HostContractViolation {
    message: string,
    class_name: string?,
    language: string?,
}

/// Raised when an `int` arithmetic operation (`+`, `-`, `*`, `/`, unary `-`)
/// overflows the representable range. `int` is a 63-bit signed integer
/// (`[-2^62, 2^62 - 1]`); operations that would fall outside it throw this
/// rather than silently wrapping. Use `bigint` for unbounded integers.
class IntegerOverflow {
    message: string,
}

type Panic = DivisionByZero
    | IndexOutOfBounds
    | InvalidFieldAccess
    | MapKeyNotFound
    | StackOverflow
    | AssertionFailed
    | Unreachable
    | Cancelled
    | UserPanic
    | Exit
    | AllocFailure
    | HostUnavailable
    | NegativeBitShift
    | SdkPanic
    | HostContractViolation
    | IntegerOverflow;
