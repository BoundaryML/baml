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

class StepBudgetExceeded {
    steps: int,
    implements Failure {
        function retry_safety(self) -> RetrySafety {
            RetrySafety.Safe
        }
    }
}

class ToolFailedError {
    id: string,
    name: string,
    message: string,
    cause: Failure?, // the classified original, when the tool threw one
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
