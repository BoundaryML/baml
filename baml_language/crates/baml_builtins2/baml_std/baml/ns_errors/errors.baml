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

class HostCallable {
  message string
  class_name string
  language string
  traceback string?
  // Opaque slot reserved for the bridge SDK's same-host rehydration
  // handle. When set by the originating host, the BAML→host decoder on
  // the same runtime can resolve it back to the original native
  // exception object; foreign runtimes / released keys see `null` and
  // fall back to the metadata fields above. Not yet populated end-to-end
  // (HostValueKind::Error wiring is pending).
  _handle $rust_type
}

/// A catch-all SDK-boundary failure with no more specific class. Synthesized
/// host-side from `RuntimeError::Other` so a call-setup failure crosses the
/// boundary as structured data rather than a stringified error.
class GenericSdkError {
  message string
}

/// A BAML compilation failure surfaced at the call boundary. Synthesized
/// host-side from `RuntimeError::Compilation`.
class CompilationError {
  message string
}
