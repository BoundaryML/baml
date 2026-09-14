/// An argument is out of range or otherwise invalid.
class InvalidArgument {
    message: string,
}

/// Thrown when text cannot be parsed into the requested type — e.g.
/// `int.parse("abc")` or `float.parse("")`.
class ParseError {
    message: string,
}

/// An I/O operation failed (file, network, etc.).
class Io {
    message: string,
}

/// An operation exceeded its time limit.
class Timeout {
    message: string,
    duration_ms: int?,
}

/// The requested operation is not supported in this context.
class Unsupported {
    message: string,
}

/// Access to a resource was denied.
class AccessError {
    message: string,
}

/// A prompt template could not be rendered.
class RenderPrompt {
    message: string,
}

/// An LLM client request failed.
class LlmClient {
    message: string,
}

class HostCallable {
    message: string,
    class_name: string,
    language: string,
    traceback: string?,
    // Opaque slot carrying the bridge SDK's same-host rehydration handle.
    // Always populated on the wire by the originating bridge — a `HostCallable`
    // with no `_handle` is not a valid BAML value. The originating bridge
    // can resolve the handle back to the original native exception object
    // (same-process round-trip → `raised === caught` identity); foreign
    // runtimes receive the same handle on the wire but their local lookup
    // misses, at which point the host SDK degrades to constructing an
    // exception from the metadata fields above. The fallback is a
    // *lookup-result* concern on the host side, not a wire shape — the
    // field itself is required.
    _handle: $rust_type,
}

/// A catch-all SDK-boundary failure with no more specific class. Synthesized
/// host-side from `RuntimeError::Other` so a call-setup failure crosses the
/// boundary as structured data rather than a stringified error.
class GenericSdkError {
    message: string,
}

/// A BAML compilation failure surfaced at the call boundary. Synthesized
/// host-side from `RuntimeError::Compilation`.
class CompilationError {
    message: string,
}

/// A value/type mismatch at the call boundary — a caller passed an argument
/// that doesn't fit the callee's (possibly inferred) type, a generic `TypeVar`
/// could not be inferred and must be specified, or repeat occurrences of a
/// `TypeVar` have no consistent binding. Synthesized host-side from
/// `EngineError::TypeMismatch`; each host SDK surfaces it as its native
/// type-error (Python `TypeError`). The stdlib itself never throws this class
/// from BAML source (reflection's typed reads throw
/// `reflect.errors.TypeMismatch` instead); user code remains free to.
class TypeMismatch {
    message: string,
}
