// scenario 27 — background jobs: ../../../../../llm-provider/ideas/scenarios/27-background-jobs/README.md
//   implementation.baml — LIBRARY-author code (the offline fake + custom shapes).
//
// The design's fourth request lifecycle: NOT one blocking response (sync), NOT one held
// connection (streaming), NOT a live duplex session (realtime). Background+poll is:
//   1. `submit` returns an OPAQUE job id IMMEDIATELY, holding no connection.
//   2. the work runs server-side; the client process may die.
//   3. you come back later, BY ID, and `poll` until the job lands.
// The job handle is persistable + idempotency-keyed, and because a submit commits an
// external effect (a billed server-side job) the provider is EFFECTFUL (D2) — a blind
// re-drive double-spends, so combinators must refuse-then-error.
//
// On the real surface the verbs are `baml.ai.Background.submit<T>(prompt, idempotency_key)
// -> baml.ai.Job<T>` and `poll<T>(job) -> T?` (null == still running). The flagship live
// provider is `baml.ai.OpenAiResponses` (`background: true` on /v1/responses). This file
// supplies the OFFLINE tier: a scripted Background provider that needs no network, so the
// design invariants — submit-now/poll-later (pending -> done), handle-as-data (persist an
// id, not bytes), idempotency-keyed dedupe, the owner guard, and the effect marker — are
// all deterministically testable. New decls carry the `s27_` prefix (shared namespace).


// A scripted, server-side-job provider. Being a value class it has no real server, so it
// simulates the lifecycle the only honest way a fake can: `remaining_polls` counts how many
// polls report "still running" before the job lands (mutated in place across polls, exactly
// like RoundRobin's cursor — the fake's stand-in for server-authoritative status). The job
// id is DERIVED from the idempotency key, so a retried submit with the same key mints the
// SAME id — the design's "idempotency-keyed" dedupe, made testable without a network.
class s27_ScriptedJobs {
    // The owner tag stamped onto every minted handle. A handle carrying any other owner was
    // minted elsewhere and MUST be refused on poll (a job id is meaningless to another
    // provider — the D2 owner guard, mirroring scenario 20's chain handles).
    owner: string,
    // Polls remaining before the job reaches a terminal, landed state. Decrements on each
    // poll while > 0 (returning null == "still running server-side"); at 0 the value lands.
    remaining_polls: int,

    implements baml.ai.Provider {
        // Submitting commits a billed, server-side job; a blind re-drive double-spends, so
        // this provider is effectful (mirrors OpenAiResponses). Combinators consult this and
        // refuse-then-error (typed CannotRetry) instead of silently re-submitting (plan D2).
        function is_effectful(self) -> bool throws never {
            true
        }
    }

    implements baml.ai.Background {
        // Create the job and return IMMEDIATELY with an opaque, persistable handle — no held
        // connection. The id is derived from the caller-owned idempotency key so a retried
        // submit with the same key coalesces onto the same job rather than spawning a second.
        function submit<T>(self, prompt: string, idempotency_key: string) -> baml.ai.Job<T>
            throws baml.errors.UnknownError {
            baml.ai.Job<T> { id: "job-" + idempotency_key, owner: self.owner }
        }

        // Read server-authoritative status BY ID. A fresh look that depends on nothing the
        // submitting process held — this is what makes a job survive a client restart. Returns
        // null while the job is still running; the typed value once it lands. Owner-guarded: a
        // handle minted by a different provider is refused typed (D2), never silently polled.
        function poll<T>(self, job: baml.ai.Job<T>) -> T?
            throws baml.errors.UnknownError {
            if (job.owner != self.owner) {
                throw baml.errors.UnknownError {
                    data: job.owner,
                    message: ["foreign job handle owned by '" + job.owner + "'; this provider ('" + self.owner + "') did not mint it and cannot poll it"],
                };
            }
            if (self.remaining_polls > 0) {
                // Still running server-side. Count this look and report "pending" (null).
                self.remaining_polls = self.remaining_polls - 1;
                return null;
            }
            // Terminal: the job has landed. Lift the typed value (the "server result").
            baml.sap.parse<T>("done " + job.id) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["scripted job result parse failed"] },
            }
        }
    }
}
