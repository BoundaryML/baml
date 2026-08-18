// The failure taxonomy. Errors carry facts; callers make judgments.
// Mirrors _planv2/pages/04_reference/03_errors.md.

enum RetrySafety {
    Safe, // failed before anything could commit
    Unknown, // request sent, outcome never observed
    Unsafe, // effects happened; never blind-replay
}

interface Failure {
    function retry_safety(self) -> RetrySafety throws never
}

// The closed error channel of the client surface: every typed failure the
// stack can produce, the two host signals (deadline, cancel) that must
// survive each boundary untouched, and schema-compilation failures from
// output-format rendering. `throws unknown` is not used on this surface;
// stored callback positions declare this alias explicitly (TYPE_SYSTEM.md
// rule 5) and parameter positions infer per call site.
type InvokeError = Failure
    | baml.errors.Timeout
    | baml.errors.UnknownError
    | baml.panics.Cancelled
    | baml.reflect.errors.CompilationError;

// Every raw type the client stack is known to raise before classification.
// This union is deliberately CLOSED: when a new throw appears anywhere in
// the provider stack, the call into `normalize` stops compiling and the new
// type must be added here with an explicit classification below.
type RaisedInClient = InvokeError
    | baml.errors.InvalidArgument
    | baml.errors.Io
    | baml.errors.ParseError
    | baml.json.JsonSerializationError
    | baml.json.JsonParseError
    | baml.json.JsonDecodeError
    | baml.errors.AccessError
    | baml.errors.Unsupported;

// The classifying gate at every client boundary. Nothing is wrapped in
// UnknownError here: each raw infrastructure type maps to a typed Failure,
// and UnknownError only passes through when a host boundary already
// produced one. Classifications:
//   InvalidArgument / ParseError / JsonSerializationError / AccessError /
//   Unsupported -> InvalidRequest
//     (request construction or client configuration bug; not retryable)
//   Io -> NetworkFailure (transport-adjacent; safely retryable)
//   JsonParseError / JsonDecodeError -> NetworkFailure
//     (malformed provider wire payload; safely retryable)
//   Timeout / Cancelled / CompilationError / Failure / UnknownError -> as-is
function normalize(error: RaisedInClient) -> InvokeError throws never {
    match (error) {
        let failure: Failure => failure,
        let unknown_error: baml.errors.UnknownError => unknown_error,
        let timeout: baml.errors.Timeout => timeout,
        let cancelled: baml.panics.Cancelled => cancelled,
        let compilation: baml.reflect.errors.CompilationError => compilation,
        let invalid: baml.errors.InvalidArgument => {
            InvalidRequest {
                provider: "client",
                status_code: null,
                detail: invalid.message,
                raw_body: null,
            }
        },
        let parse: baml.errors.ParseError => {
            InvalidRequest {
                provider: "client",
                status_code: null,
                detail: parse.message,
                raw_body: null,
            }
        },
        let serialize: baml.json.JsonSerializationError => {
            InvalidRequest {
                provider: "client",
                status_code: null,
                detail: serialize.message,
                raw_body: null,
            }
        },
        let access: baml.errors.AccessError => {
            InvalidRequest {
                provider: "client",
                status_code: null,
                detail: access.message,
                raw_body: null,
            }
        },
        let unsupported: baml.errors.Unsupported => {
            InvalidRequest {
                provider: "client",
                status_code: null,
                detail: unsupported.message,
                raw_body: null,
            }
        },
        let io: baml.errors.Io => {
            NetworkFailure { provider: "client", detail: io.message, raw_body: null }
        },
        let json_parse: baml.json.JsonParseError => {
            NetworkFailure { provider: "client", detail: json_parse.message, raw_body: null }
        },
        let json_decode: baml.json.JsonDecodeError => {
            NetworkFailure { provider: "client", detail: json_decode.message, raw_body: null }
        },
    }
}

// Kept as a named boundary for the invoke-path call sites; identical gate.
function normalize_invoke(error: RaisedInClient) -> InvokeError throws never {
    normalize(error)
}

class RateLimited {
    provider: string,
    retry_after_ms: int?,
    raw_body: string?,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

class NetworkFailure {
    provider: string,
    detail: string,
    raw_body: string?,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

class InvalidRequest {
    provider: string,
    status_code: int?,
    detail: string,
    raw_body: string?,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

// A request preview cannot represent this input without performing the I/O
// that the preview API explicitly forbids (for example, reading a local media
// file or downloading a URL for a provider that has no URL wire form).
class PreviewUnsupported {
    provider: string,
    detail: string,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

class Refused {
    provider: string,
    reason: string,
    raw_body: string?,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

class ParseFailed {
    provider: string,
    raw_output: string,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

// The provider completed a turn with a terminal reason that is not a valid
// successful completion. This stays distinct from ParseFailed: the response
// may be syntactically valid but incomplete because the model exhausted its
// output budget. Host SDKs can map MaxTokens to their legacy `length` finish
// reason without treating it as a schema-validation failure.
class FinishReasonError {
    provider: string,
    finish_reason: root.content.StopReason,
    raw_output: string,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

// The runner hit its step budget. Unlike the client-layer failures above,
// this is thrown AFTER turns were committed to the journal — and any tools
// those turns called have already run. Local state advanced, so a blind
// replay is not safe.
class StepBudgetExceeded {
    steps: int,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Unsafe
        }
    }
}

// A tool failed and the run was configured to raise. Thrown after the
// ToolFailed event is on the record, and a tool is arbitrary user code that
// may have committed effects before it failed — so this is never blind-
// replayable, whatever the underlying `cause` reports.
class ToolFailedError {
    id: string,
    name: string,
    message: string,
    cause: Failure?, // the classified original, when the tool threw one
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Unsafe
        }
    }
}

// The selected client cannot stream: it does not implement
// ai.StreamingClient. Thrown by ai.from_spec when the spec's default
// client lacks the capability (an explicit override is caught at the call
// site by its StreamingClient type).
class StreamingUnsupported {
    client_id: string,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

// HTTP status -> classified failure. The classifier attaches the body it
// received as raw_body, so a failure always carries what the provider said.
//
// `headers` is the response's header map when the caller has it (ai.wire.send_as
// passes `resp.headers`); it is what fills RateLimited.retry_after_ms. It
// defaults to null so a caller that only has a status and a body still compiles
// — such a call just cannot report a retry delay.
function classify_http(
    provider: string,
    status_code: int,
    body: string,
    headers: map<string, string>? = null,
) -> Failure {
    if (status_code == 429) {
        RateLimited {
            provider: provider,
            retry_after_ms: root.internal._retry_after_ms(headers),
            raw_body: body,
        }
    } else if (status_code == 408 || status_code >= 500) {
        NetworkFailure { provider: provider, detail: `http ${status_code}`, raw_body: body }
    } else {
        InvalidRequest {
            provider: provider,
            status_code: status_code,
            detail: `http ${status_code}`,
            raw_body: body,
        }
    }
}
