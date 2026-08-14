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

// Close an untyped error channel at an ai boundary. Classified failures and
// already-normalized unknown errors retain their identity; everything else is
// captured in the standard untyped wrapper.
function normalize(error: unknown) -> Failure | baml.errors.UnknownError throws never {
    match (error) {
        let failure: Failure => failure,
        let unknown_error: baml.errors.UnknownError => unknown_error,
        _ => baml.errors.UnknownError { data: error, message: [error.to_string()] },
    }
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
function classify_http(provider: string, status_code: int, body: string) -> Failure {
    if (status_code == 429) {
        RateLimited { provider: provider, retry_after_ms: null, raw_body: body }
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
