/// Raised when an integer is divided by zero.
class DivisionByZero {
  dividend int
}

/// Raised when an array or string is accessed with an out-of-bounds index.
class IndexOutOfBounds {
  index int
  length int
}

/// Raised when a map is accessed with a key that does not exist.
class MapKeyNotFound {
  key string
}

/// Raised when the call stack depth limit is exceeded.
class StackOverflow {
  message string
}

/// Raised when an `assert` statement fails.
class AssertionFailed {
  message string
}

/// Raised when execution reaches a branch that should be impossible.
class Unreachable {
  message string
}

/// A cancellation was requested.
class Cancelled {
  message string
}

/// A user-caused panic from `baml.sys.panic`.
class UserPanic {
  message string
}

/// A clean process-termination request from `baml.sys.exit(code)`.
///
/// Catchable like any other panic; if left unhandled, the engine
/// terminates the process with this code. Patterned after Python's
/// `SystemExit`.
class Exit {
  code int
}

/// Memory allocation failure. This happens when an operation would have caused an unrecoverable
/// Out-Of-Memory error so we panic instead. Note that not all memory allocation failures are
/// guaranteed to panic; some may cause a hard failure.
class AllocFailure {
  message string
}

/// A required host resource is unavailable — e.g. the system entropy source
/// (`getrandom`) is not accessible in a sandboxed/isolated environment.
/// `resource` identifies which subsystem is unavailable (`"entropy"`, etc.).
class HostUnavailable {
  resource string
  message string
}

type Panic = DivisionByZero | IndexOutOfBounds | MapKeyNotFound | StackOverflow | AssertionFailed | Unreachable | Cancelled | UserPanic | Exit | AllocFailure | HostUnavailable
