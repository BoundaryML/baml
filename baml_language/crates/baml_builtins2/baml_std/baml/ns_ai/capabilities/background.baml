// Async background jobs (scenario 27): submit now, poll later — the job handle is
// persistable and idempotency-keyed. Live implementation: providers/openai_responses.baml
// (`background: true` on /v1/responses).

/// A persistable handle to a submitted background job.
class Job<T> {
    id: string,
    owner: string,
}

/// Submit-and-poll background execution. Submitting commits an external effect
/// (a billed server-side job) — implementors are effectful (D2).
interface Background requires root.ai.Provider {
    function submit<T>(self, prompt: string, idempotency_key: string) -> Job<T> throws root.errors.UnknownError
    function poll<T>(self, job: Job<T>) -> T? throws root.errors.UnknownError
}
