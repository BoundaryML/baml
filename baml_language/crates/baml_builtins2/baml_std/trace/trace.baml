/// Tracing mode for one invocation, not its descendants.
enum Mode {
    Hidden,
    Timing,
    Span,
}

/// Per-invocation capture overrides: true captures, false disables capture,
/// and null inherits automatic behavior. Explicit values override function policy.
class Capture {
    inputs: bool?,
    output: bool?,
    error: bool?,
}

/// Detached configuration, not resolved tracing policy.
class Settings {
    mode: Mode?,
    capture: Capture,
    context: ContextOptions,
}

/// Detached effective context. Metadata contains no removal markers.
class Context {
    distinct_id: string?,
    metadata: map<string, string | int | float | bool>,
}

/// A detached patch; null metadata values remove keys at invocation entry.
class ContextOptions {
    distinct_id: string?,
    metadata: map<string, string | int | float | bool | null>,
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

    /// Compose a shallow patch without reading ambient context.
    //baml:mut_vm
    function context(
        self,
        metadata: map<string, string | int | float | bool | null> = {},
        distinct_id: string? = null,
    ) -> Options throws never {
        $rust_function
    }

    /// Change only mode. Null restores the function's default mode.
    //baml:mut_vm
    function mode(self, mode: Mode?) -> Options throws never {
        $rust_function
    }

    /// Select Span mode and replace all three capture overrides.
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

/// Record this invocation individually, with optional value capture overrides.
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

//baml:mut_vm
function context(
    metadata: map<string, string | int | float | bool | null> = {},
    distinct_id: string? = null,
) -> Options throws never {
    $rust_function
}

/// Read a detached snapshot of the current invocation, even with tracing off.
//baml:mut_vm
function current_context() -> Context throws never {
    $rust_function
}

/// Update this invocation and subsequently launched descendants, not its caller.
//baml:mut_vm
function update_context(
    metadata: map<string, string | int | float | bool | null> = {},
    distinct_id: string? = null,
) -> void throws never {
    $rust_function
}

/// (internal) The span or thread that model usage recorded later belongs
/// to: this invocation's innermost span, else its thread. 0 when tracing is
/// off. Model clients capture it when a call starts.
//baml:mut_vm
function _usage_target() -> int throws never {
    $rust_function
}

/// (internal) Record the tokens one model turn used against a target from
/// `_usage_target`. `input_tokens` excludes cache reads and writes. Returns
/// whether anything was recorded.
//baml:mut_vm
function _record_usage(
    target: int,
    model: string?,
    input_tokens: int,
    output_tokens: int,
    cache_read_tokens: int?,
    cache_write_tokens: int?,
    reasoning_tokens: int?,
) -> bool throws never {
    $rust_function
}
