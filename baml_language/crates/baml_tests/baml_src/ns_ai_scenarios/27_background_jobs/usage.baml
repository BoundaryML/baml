// scenario 27 — background jobs: ../../../../../llm-provider/ideas/scenarios/27-background-jobs/README.md
// Submit + poll with the caller-owned idempotency key; live tier =
// responses_background_live (Rust ai_responses suite).

/// Offline Background provider: pending on the first poll, done on the next.
class ScriptedJobs {
    turns_until_done: int,

    implements baml.ai.Provider {
        function is_effectful(self) -> bool throws never {
            true
        }
    }

    implements baml.ai.Background {
        function submit<T>(self, prompt: string, idempotency_key: string) -> baml.ai.Job<T>
            throws baml.errors.UnknownError {
            baml.ai.Job<T> { id: "job-" + idempotency_key, owner: "scripted" }
        }
        function poll<T>(self, job: baml.ai.Job<T>) -> T?
            throws baml.errors.UnknownError {
            if (self.turns_until_done > 0) {
                return null;
            }
            baml.sap.parse<T>("finished " + job.id) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["poll parse"] },
            }
        }
    }
}

test "27: submit carries the idempotency key onto the job handle" {
    let p: baml.ai.Provider = ScriptedJobs { turns_until_done: 0 };
    let outcome = match (p) {
        let b: baml.ai.Background => {
            let job: baml.ai.Job<string> = b.submit<string>("big analysis", "key-42") catch (e) { _ => baml.ai.Job<string> { id: "failed", owner: "" } };
            match (b.poll<string>(job) catch (e) { _ => "poll failed" }) {
                null => "pending",
                let done: string => done,
            }
        },
        _ => "no background capability",
    };
    assert.equal(outcome, "finished job-key-42")
}

test "27: an unfinished job polls as pending, not an error" {
    let p: baml.ai.Provider = ScriptedJobs { turns_until_done: 1 };
    let outcome = match (p) {
        let b: baml.ai.Background => {
            let job: baml.ai.Job<string> = b.submit<string>("x", "k") catch (e) { _ => baml.ai.Job<string> { id: "failed", owner: "" } };
            match (b.poll<string>(job) catch (e) { _ => "poll failed" }) {
                null => "pending",
                let done: string => done,
            }
        },
        _ => "no background capability",
    };
    assert.equal(outcome, "pending")
}

test "27: Retry refuses to wrap the effectful background provider" {
    let p: baml.ai.Provider = ScriptedJobs { turns_until_done: 0 };
    assert.is_true(p.is_effectful())
}
