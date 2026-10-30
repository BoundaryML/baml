/// Tracing mode for one invocation, not its descendants.
enum Mode {
    Hidden,
    Timing,
    Span,
}

/// Per-invocation capture requests: true requests capture; false and null add
/// no request. Applicable function-policy requests still capture; false is not
/// a privacy veto. Requests do not propagate to descendant invocations.
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
    @@rust_type("SpanId")
}

/// Immutable, reusable tracing options. Builders leave their receiver unchanged.
class Options {
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

    /// Select Span mode and replace all three capture requests.
    /// Capture outputs and errors by default; inputs require an explicit request.
    function span(
        self,
        inputs: bool? = null,
        output: bool? = true,
        error: bool? = true,
    ) -> Options throws never {
        self._span(inputs, output, error)
    }

    /// Select Span mode without requesting value capture.
    /// Applicable function-policy requests still capture values.
    function empty_span(self) -> Options throws never {
        self._span(false, false, false)
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

    @@rust_type("TraceOptions")
}

/// A single-use reservation. Inspection remains available after attachment.
class ReservedSpan {
    //baml:mut_vm
    function id(self) -> SpanId throws never {
        $rust_function
    }

    /// Return a detached snapshot of the reserved configuration.
    //baml:mut_vm
    function inspect(self) -> Settings throws never {
        $rust_function
    }

    @@rust_type("ReservedSpan")
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

/// Record this invocation individually, capturing outputs and errors by default.
/// Inputs require an explicit request.
function span(
    inputs: bool? = null,
    output: bool? = true,
    error: bool? = true,
) -> Options throws never {
    _span(inputs, output, error)
}

/// Record this invocation individually without requesting value capture.
/// Applicable function-policy requests still capture values.
function empty_span() -> Options throws never {
    _span(false, false, false)
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

/// Apply a context patch to a call. Use an IIFE for a smaller scope:
///
/// ```baml
/// (() -> {
///     log.info("Processing payment")
/// })($trace = trace.context(metadata = { "phase": "payment" }))
/// ```
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

/// An invalid span target, parent, or activation of a closed span.
class SpanError {
    message: string,
}

/// An operation whose lifetime the code that starts it owns, instead of one
/// function call's: for work that begins in one call and ends in another, such
/// as a stream that is pulled step by step, or an agent run spanning many
/// model turns. It is a span like a traced function call's, nested under the
/// span active where it starts (or `parent`).
///
/// Start it with `trace.start_span`, run each piece of its work inside it with
/// `run`, and end it once with `finish`, `fail`, `fail_with_context` or
/// `close`. Inputs are captured at creation. The first ending wins; `close`
/// makes no claim about the outcome.
class Span {
    _handle: $rust_type,

    //baml:mut_vm
    function id(self) -> SpanId? throws never {
        $rust_function
    }

    //baml:mut_vm
    function is_closed(self) -> bool throws never {
        $rust_function
    }

    /// Record a successful output and end the operation.
    //baml:mut_vm
    function finish<T>(self, value: T) -> void throws never {
        $rust_function
    }

    /// Record an error value and end the operation. Does not throw that value.
    //baml:mut_vm
    function fail<E>(self, error: E) -> void throws never {
        $rust_function
    }

    /// Record an error with its existing baml.errors.Context and end the operation.
    //baml:mut_vm
    //baml:fallible
    function fail_with_context<E>(
        self,
        error: E,
        context: baml.errors.Context,
    ) -> void throws never {
        $rust_function
    }

    /// End without asserting success, failure, or cancellation.
    //baml:mut_vm
    function close(self) -> void throws never {
        $rust_function
    }

    /// Execute with this operation as the parent, using its context without completing it.
    //baml:trace_passthrough
    function run<T, E>(self, body: () -> T throws E) -> T throws E | SpanError {
        self._activate();
        body()
    }

    /// (internal) Make this operation the active span for the rest of the
    /// calling frame, which `run` scopes to its body.
    //baml:mut_vm
    function _activate(self) -> void throws SpanError {
        $rust_function
    }
}

/// Start an operation associated with a name or an existing function value.
/// Creating it does not invoke the function or change the caller's active span.
//baml:mut_vm
function start_span(
    target: string | reflect.AnyFunction,
    inputs: map<string, unknown>? = null,
    parent: Span? = null,
    metadata: map<string, string | int | float | bool | null> = {},
    distinct_id: string? = null,
) -> Span throws SpanError {
    $rust_function
}

/// Internal: read this pass-through invocation's effective tracing controls.
//baml:mut_vm
//baml:fallible
function _passthrough() -> Options | ReservedSpan throws never {
    $rust_function
}

/// Internal: create the operation using an already-claimed reservation, if any.
//baml:mut_vm
function _create_span(
    target: string | reflect.AnyFunction,
    inputs: map<string, unknown>,
    config: Options | ReservedSpan,
) -> Span throws SpanError {
    $rust_function
}
