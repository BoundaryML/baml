/// Raised when an `int` or `bigint` is divided by zero, by either `/` or `%`.
///
/// `float` division does not raise this: IEEE-754 has values for the result,
/// so `1.0 / 0.0` is `inf` and `0.0 / 0.0` is `NaN`.
///
/// # Examples
///
/// ```baml
/// let result = 1 / 0; // panics
/// ```
class DivisionByZero {
    /// The left operand, in whichever integer type the division used.
    dividend: int | bigint,
}

/// Raised when an array or string is accessed with an out-of-bounds index.
///
/// # Examples
///
/// ```baml
/// let arr = [1, 2, 3];
/// let result = arr[5]; // panics
/// ```
class IndexOutOfBounds {
    index: int,
    length: int,
}

/// Raised when VM field access targets a stale or invalid field slot.
/// TODO: this should be an internal vm error, NOT a user-facing panic.
class InvalidFieldAccess {
    field_index: int,
    field_count: int,
}

/// Raised when a map is accessed with a key that does not exist.
///
/// # Examples
///
/// ```baml
/// let map = { "key": 42 };
/// let result = map["nonexistent"]; // panics
/// ```
class MapKeyNotFound {
    key: string,
}

/// Raised when the call stack depth limit is exceeded.
///
/// # Examples
///
/// ```baml
/// function infinite_recursion() -> void {
///     infinite_recursion(); // panics
/// }
/// ```
class StackOverflow {
    message: string,
}

/// Raised when an `assert` statement fails.
///
/// # Examples
///
/// ```baml
/// assert.equal(1, 2); // panics
/// ```
class AssertionFailed {
    message: string,
}

/// Used to mark a branch that should be impossible to reach,
/// based on user invariants that cannot be validated by the compiler.
///
/// # Examples
///
/// TODO: `unreachable()` needs to be added back as an alternative to `panic` and `exit`
class Unreachable {
    message: string,
}

/// A cancellation was requested.
///
/// # Examples
///
/// ```baml
/// let fut = spawn {
///     baml.sys.sleep(baml.time.Duration.from_seconds(5)); // panics
/// };
/// fut.cancel();
/// await fut; // panics (propogated)
/// ```
class Cancelled {
    message: string,
}

/// A user-caused panic from `baml.sys.panic`.
///
/// # Examples
///
/// ```baml
/// baml.sys.panic("something went wrong");
/// ```
class UserPanic {
    message: string,
}

/// A clean process-termination request from `baml.sys.exit(code)`.
///
/// Catchable like any other panic; if left unhandled, the engine
/// terminates the process with this code. Patterned after Python's
/// `SystemExit`.
///
/// # Examples
///
/// ```baml
/// baml.sys.exit(0);
/// ```
class Exit {
    code: int,
}

/// Memory allocation failure. This happens when an operation would have caused an unrecoverable
/// Out-Of-Memory error so we panic instead. Note that not all memory allocation failures are
/// guaranteed to panic; some may cause a hard failure.
///
/// # Examples
///
/// ```baml
/// let too_big = uint8array.zeroes(int.max_value()); // panics
/// ```
class AllocFailure {
    message: string,
}

/// A required host resource is unavailable — e.g. the system entropy source
/// (`getrandom`) is not accessible in a sandboxed/isolated environment.
/// `resource` identifies which subsystem is unavailable (`"entropy"`, etc.).
///
/// # Examples
///
/// ```baml
/// baml.fs.open("file.txt"); // panics on browser
/// ```
class HostUnavailable {
    resource: string,
    message: string,
}

/// The right operand of a shift (`<<` / `>>`) was negative.
/// The type system can't rule this out because the shift count is a runtime value.
///
/// # Examples
///
/// ```baml
/// let result = 4 << -1; // panics
/// ```
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
///
/// # Examples
///
/// ```baml
/// let result = int.max_value() + 1; // panics
/// ```
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
