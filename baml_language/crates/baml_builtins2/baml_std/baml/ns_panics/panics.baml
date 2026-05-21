class DivisionByZero {
  dividend int
}

class IndexOutOfBounds {
  index int
  length int
}

class MapKeyNotFound {
  key string
}

class StackOverflow {
  message string
}

class AssertionFailed {
  message string
}

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
