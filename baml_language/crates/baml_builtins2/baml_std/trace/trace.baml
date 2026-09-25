/// Tracing mode for one invocation, not its descendants.
enum Mode {
    Hidden,
    Timing,
    Span,
}

/// Capture requests: null uses automatic behavior; false adds no request.
/// Function-policy requests still apply, so false is not a privacy veto.
class Capture {
    inputs: bool?,
    output: bool?,
    error: bool?,
}

/// Detached configuration, not resolved tracing policy.
class Settings {
    mode: Mode?,
    capture: Capture,
}

/// An opaque, immutable, globally scoped span identity.
/// Equality includes the recording scope, not just its local span number.
/// There is no public constructor, component access, or serialization API.
class SpanId {
    _handle: $rust_type,
}

/// Immutable, reusable tracing options. Builders leave their receiver unchanged.
class Options {
    _handle: $rust_type,

    /// Change only mode. Null restores the function's default mode.
    //baml:mut_vm
    function mode(self, mode: Mode?) -> Options throws never {
        $rust_function
    }

    /// Select Span mode and replace all three capture requests.
    function span(
        self,
        inputs: bool? = null,
        output: bool? = null,
        error: bool? = null,
    ) -> Options throws never {
        self._span(inputs, output, error)
    }

    //baml:mut_vm
    function _span(self, inputs: bool?, output: bool?, error: bool?) -> Options throws never {
        $rust_function
    }

    /// Return a detached snapshot. Mutating it does not change these options.
    //baml:mut_vm
    function inspect(self) -> Settings throws never {
        $rust_function
    }

    /// Snapshot these options, select Span mode, and reserve one invocation's ID.
    /// The reservation can cross threads within this runtime, not into another runtime.
    /// Reattaching the same reservation, including through an alias, is an error.
    //baml:mut_vm
    function reserve(self) -> ReservedSpan throws never {
        $rust_function
    }
}

/// A single-use reservation. Inspection remains available after attachment.
class ReservedSpan {
    _handle: $rust_type,

    //baml:mut_vm
    function id(self) -> SpanId throws never {
        $rust_function
    }

    /// Return a detached snapshot of the reserved configuration.
    //baml:mut_vm
    function inspect(self) -> Settings throws never {
        $rust_function
    }
}

function options(mode: Mode? = null) -> Options throws never {
    _options(mode)
}

//baml:mut_vm
function _options(mode: Mode?) -> Options throws never {
    $rust_function
}

/// Omit this invocation. Descendants can still be traced.
//baml:mut_vm
function hidden() -> Options throws never {
    $rust_function
}

/// Collect timing and outcome statistics without forcing an individual span.
//baml:mut_vm
function timing() -> Options throws never {
    $rust_function
}

/// Record this invocation individually, with optional value capture requests.
function span(
    inputs: bool? = null,
    output: bool? = null,
    error: bool? = null,
) -> Options throws never {
    _span(inputs, output, error)
}

//baml:mut_vm
function _span(inputs: bool?, output: bool?, error: bool?) -> Options throws never {
    $rust_function
}

/// Return this invocation's own span ID, never an ancestor's.
/// Hidden, timing-only, and tracing-disabled invocations return null.
/// Reading the ID does not create or promote a span.
//baml:mut_vm
function current_span_id() -> SpanId? throws never {
    $rust_function
}
