/// An argument is out of range or otherwise invalid.
class InvalidArgument {
  message string
}

/// Thrown when text cannot be parsed into the requested type — e.g.
/// `int.parse("abc")` or `float.parse("")`.
class ParseError {
  message string
}

/// An I/O operation failed (file, network, etc.).
class Io {
  message string
}

/// An operation exceeded its time limit.
class Timeout {
  message string
  duration_ms int?
}

/// The requested operation is not supported in this context.
class Unsupported {
  message string
}

/// Access to a resource was denied.
class AccessError {
  message string
}

/// A prompt template could not be rendered.
class RenderPrompt {
  message string
}

/// The operation has not been implemented.
class NotImplemented {
  message string
}

/// An LLM client request failed.
class LlmClient {
  message string
}

/// A catch-all error for development-time issues.
class DevOther {
  message string
}

/// The host runtime panicked.
class HostPanic {
  message string
}
