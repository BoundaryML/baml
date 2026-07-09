// scenario 27 — background jobs: ../../../../../llm-provider/ideas/scenarios/27-background-jobs/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design's fourth request lifecycle: `submit` returns an opaque job id NOW, the work
// runs server-side while the client may die, and you come back BY ID and `poll` until it
// lands. The job handle is persistable + idempotency-keyed; and because a submit commits a
// billed server-side effect, the provider is EFFECTFUL (D2) — a blind re-drive double-spends.
// On the real surface the verbs are `baml.ai.Background.submit<T>(prompt, idempotency_key)`
// and `poll<T>(job) -> T?`, used DIRECTLY (that IS the lesson of this scenario — there is no
// `.background` companion; a durable, resumable id cannot be faked by a one-shot call).
//
// This file exercises, per the design:
//
//   OFFLINE (s27_ScriptedJobs, deterministic — implementation.baml):
//     - submit-now/poll-later: submit returns a handle immediately; poll goes pending -> done.
//     - idempotency-keyed: the same key mints the SAME job id (a retried submit dedupes).
//     - handle-as-data: a Job<T> survives a string round-trip (persist an id, not bytes) and
//       rehydrates in a "later process" to poll the same job.
//     - the D2 owner guard: polling a handle minted by a DIFFERENT provider fails typed.
//     - the effect marker refusing Retry: wrapping the effectful provider in the generic
//       Retry and driving the call path REFUSES with typed CannotRetry (never silent).
//     - negotiation: a non-Background provider falls to the _ arm (no faked job id).
//
//   LIVE (testset "integ-test-27-background-jobs", real OpenAI /v1/responses):
//     - real background submit (`background: true`) returns a job id immediately; poll a
//       fresh GET /responses/{id} until status == completed and the typed value lands.


// ── OFFLINE TIER ────────────────────────────────────────────────────────────────

test "27: submit returns a handle now; polling goes pending -> done" {
    // remaining_polls: 1 means the first poll reports "still running server-side" and the
    // second lands the value — a genuine lifecycle transition on a stateless fake.
    let jobs = s27_ScriptedJobs { owner: "scripted", remaining_polls: 1 };
    let p: baml.ai.Provider = jobs;
    let outcome = match (p) {
        let b: baml.ai.Background => {
            // submit returns IMMEDIATELY — no connection held, just the opaque handle.
            let job: baml.ai.Job<string> = b.submit<string>("big architectural review", "req-42") catch (e) {
                _ => baml.ai.Job<string> { id: "SUBMIT_FAILED", owner: "" },
            };
            log.info("27 submit -> job id: " + job.id);
            let first = match (b.poll<string>(job) catch (e) { _ => "poll-error" }) {
                null => "pending",
                let v: string => v,
            };
            log.info("27 poll #1: " + first);
            let second = match (b.poll<string>(job) catch (e) { _ => "poll-error" }) {
                null => "pending",
                let v: string => v,
            };
            log.info("27 poll #2: " + second);
            first + " | " + second
        },
        _ => "no background capability",
    };
    // pending on the first look, the landed value on the second.
    assert.equal(outcome, "pending | done job-req-42")
}

test "27: submit is idempotency-keyed — the same key mints the same job" {
    // The APP owns the idempotency key. A retried submit with the SAME key must coalesce onto
    // the SAME server job (no double-spend); a different key is a different job.
    let jobs = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
    let p: baml.ai.Provider = jobs;
    let ids = match (p) {
        let b: baml.ai.Background => {
            let j1: baml.ai.Job<string> = b.submit<string>("review", "req-key-A") catch (e) { _ => baml.ai.Job<string> { id: "E1", owner: "" } };
            let j2: baml.ai.Job<string> = b.submit<string>("review (retried)", "req-key-A") catch (e) { _ => baml.ai.Job<string> { id: "E2", owner: "" } };
            let j3: baml.ai.Job<string> = b.submit<string>("review", "req-key-B") catch (e) { _ => baml.ai.Job<string> { id: "E3", owner: "" } };
            j1.id + " | " + j2.id + " | " + j3.id
        },
        _ => "no background capability",
    };
    log.info("27 idempotency ids: " + ids);
    // same key -> same job id (dedupe); different key -> different job id.
    assert.equal(ids, "job-req-key-A | job-req-key-A | job-req-key-B")
}

test "27: a Job handle survives a string round-trip (persist an id, not bytes)" {
    // The durability win: the app persists ONLY the handle (a small string), never a
    // transcript or connection. A later process rehydrates it and polls the SAME job.
    let jobs = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
    let p: baml.ai.Provider = jobs;
    let outcome = match (p) {
        let b: baml.ai.Background => {
            let job: baml.ai.Job<string> = b.submit<string>("long job", "persist-me") catch (e) {
                _ => baml.ai.Job<string> { id: "SUBMIT_FAILED", owner: "" },
            };
            // serialize (what a real app writes to its store) ...
            let saved: string = baml.json.to_string<baml.ai.Job<string>>(job) catch (e) { _ => "SERIALIZE_FAILED" };
            log.info("27 persisted handle: " + saved);
            // ... then rehydrate in a "later process" (SAP the string back into the handle).
            let re: baml.ai.Job<string> = baml.sap.parse<baml.ai.Job<string>>(saved) catch (e) {
                _ => baml.ai.Job<string> { id: "PARSE_FAILED", owner: "" },
            };
            // the rehydrated handle still polls to a landed value — the id was enough.
            let done = match (b.poll<string>(re) catch (e) { _ => "poll-error" }) {
                null => "pending",
                let v: string => v,
            };
            re.id + " / " + re.owner + " / " + done
        },
        _ => "no background capability",
    };
    log.info("27 round-trip outcome: " + outcome);
    assert.equal(outcome, "job-persist-me / scripted / done job-persist-me")
}

test "27: a foreign-owner job handle is refused, not silently polled (D2 owner guard)" {
    // A handle minted by OpenAI's Responses provider, smuggled into the scripted provider. A
    // job id is meaningless to another provider — poll must fail TYPED, not run against it.
    let jobs = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
    let foreign = baml.ai.Job<string> { id: "resp_openai_xyz", owner: "openai-responses" };
    let p: baml.ai.Provider = jobs;
    let outcome: string? = match (p) {
        let b: baml.ai.Background => b.poll<string>(foreign) catch (e) {
            let u: baml.errors.UnknownError => "refused: " + u.message.join("; "),
        },
        _ => "no background capability",
    };
    match (outcome) {
        null => assert.is_true(false),
        let s: string => {
            log.info("27 foreign-handle outcome: " + s);
            assert.is_true(s.starts_with("refused:"))
            assert.is_true(s.includes("openai-responses"))
        },
    }
}

test "27: the effectful provider refuses a blind Retry re-drive (typed CannotRetry)" {
    // Submitting a background job is EFFECTFUL (a billed server-side job). Wrapping the
    // provider in the generic Retry and driving the call path must REFUSE — typed
    // CannotRetry — rather than silently double-submitting (plan D2: refuse-then-error).
    let base: baml.ai.Provider = s27_ScriptedJobs { owner: "scripted", remaining_polls: 0 };
    assert.is_true(base.is_effectful())

    let guarded: baml.ai.Provider = base.with_retry(3);
    let outcome = match (guarded) {
        let h: baml.ai.HttpProvider => h.call<string>("re-drive me") catch (e) {
            let cr: baml.errors.CannotRetry => "refused: " + cr.message,
            _ => "other error",
        },
        _ => "retry is not callable",
    };
    log.info("27 retry-refusal outcome: " + outcome);
    assert.is_true(outcome.starts_with("refused:"))
    assert.is_true(outcome.includes("effectful"))
}

test "27: a non-background provider is not asked to submit (typed negotiation)" {
    // Background is a provider PROPERTY: a plain chat provider has no submit/poll, so the
    // capability negotiation falls to the _ arm rather than faking a durable, resumable id.
    let p: baml.ai.Provider = baml.ai.OpenAi { model: "gpt-4o", api_key: "unused", base_url: null };
    let outcome = match (p) {
        let b: baml.ai.Background => "unexpectedly a background provider",
        _ => "not a background provider",
    };
    assert.equal(outcome, "not a background provider")
}


// ── LIVE TIER (excluded by default; run with keys via `-i "integ-test-27-*::"`) ────
// The real background lifecycle against OpenAI /v1/responses. `submit` sends
// `background: true` and returns a job id IMMEDIATELY (the HTTP call does not block for the
// answer); `poll` is a fresh GET /responses/{id} that yields the typed value once the
// server-authoritative status reaches "completed". Tiny prompt so it lands fast.
testset "integ-test-27-background-jobs" {
    test "27 live: submit a background job, then poll until it lands" {
        let p = baml.ai.OpenAiResponses {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        // The app owns the idempotency key — a retried submit coalesces server-side.
        let key = baml.id.new();
        let job: baml.ai.Job<string> = p.submit<string>(
            "Reply with exactly one word: pong",
            key,
        ) catch (e) {
            let u: baml.errors.UnknownError => baml.ai.Job<string> { id: "", owner: "openai-responses" },
        };
        log.info("27 live submit -> job id: " + job.id + " (idem key: " + key + ")");
        // submit returns a handle immediately, holding no connection.
        assert.is_true(job.id != "")

        // Poll a fresh GET by id until the server-authoritative status lands the value.
        let landed = "";
        let attempt = 0;
        while (attempt < 25 && landed == "") {
            match (p.poll<string>(job) catch (e) {
                let u: baml.errors.UnknownError => null,
            }) {
                null => {
                    log.info("27 live poll #" + attempt.to_string() + ": pending");
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(2000)) catch (se) { _ => null };
                    attempt = attempt + 1;
                },
                let v: string => {
                    landed = v;
                    log.info("27 live poll #" + attempt.to_string() + ": DONE -> " + v);
                },
            }
        }
        assert.is_true(landed != "")
        assert.is_true(landed.to_lower_case().includes("pong"))
    }
}
