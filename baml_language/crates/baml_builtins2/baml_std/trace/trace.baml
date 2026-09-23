enum Mode {
    Hidden,
    Timing,
    Span,
}

/// Detached capture configuration. Null selects automatic behavior.
/// False adds no capture request; it does not veto function policy.
class Capture {
    inputs: bool?,
    output: bool?,
    error: bool?,
}

/// Detached configuration, not resolved policy or active invocation state.
class Settings {
    mode: Mode?,
    capture: Capture,
}

/// Immutable, reusable call configuration.
class Options {
    _handle: $rust_type,

    /// Change only the mode. Null restores the function's default.
    //baml:mut_vm
    function mode(self, mode: Mode?) -> Options throws never {
        $rust_function
    }

    /// Select Span mode and replace the capture configuration.
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

    //baml:mut_vm
    function inspect(self) -> Settings throws never {
        $rust_function
    }

    /// Snapshot these options, select Span mode, and reserve one invocation ID.
    //baml:mut_vm
    function reserve(self) -> ReservedSpan throws never {
        $rust_function
    }
}

/// A reservation may be attached once, across logical threads in this runtime.
/// Inspection and identity remain available after attachment.
class ReservedSpan {
    _handle: $rust_type,

    //baml:mut_vm
    function id(self) -> SpanId throws never {
        $rust_function
    }

    //baml:mut_vm
    function inspect(self) -> Settings throws never {
        $rust_function
    }
}

/// Opaque immutable identity. Equality compares identities, not wrappers.
class SpanId {
    _handle: $rust_type,

    //baml:vm
    function _equals(self, other: SpanId) -> bool throws never {
        $rust_function
    }

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self._equals(other)
        }
    }
}

function options(mode: Mode? = null) -> Options throws never {
    _options(mode)
}

//baml:mut_vm
function _options(mode: Mode?) -> Options throws never {
    $rust_function
}

function hidden() -> Options throws never {
    options(mode = Mode.Hidden)
}

function timing() -> Options throws never {
    options(mode = Mode.Timing)
}

function span(
    inputs: bool? = null,
    output: bool? = null,
    error: bool? = null,
) -> Options throws never {
    options(mode = Mode.Span).span(inputs = inputs, output = output, error = error)
}

/// The caller's own active span, never an ancestor's.
//baml:mut_vm
function current_span_id() -> SpanId? throws never {
    $rust_function
}
