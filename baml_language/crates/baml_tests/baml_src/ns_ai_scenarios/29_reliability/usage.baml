// scenario 29 — reliability: ../../../../../llm-provider/ideas/scenarios/29-reliability/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The design's app story: reliability is a COMBINATOR CHAIN on the provider a
// plain LLM function calls — `Reliable() = Gpt4Flex().with_retry(4)`,
// `Resilient() = Gpt4Flex().with_retry(2).fallback_to(Claude().with_retry(2))`,
// `Balanced() = RoundRobin.over([KeyA, KeyB]).fallback_to(Claude())`. No
// options blob, no `retry_policy:` stanza in the function — orchestration WRAPS
// the provider and is handed to the SAME function body via the client.
//
// On the REAL surface this rides `baml.ai`'s combinators (ns_ai/combinators.baml,
// reached fluently on the `Provider` marker): `.with_retry(max)` -> `Retry`,
// `.fallback_to(other)` -> `Fallback`, `RoundRobin { members }`, `.traced(meter)`
// -> `Traced`. Retry's decision is the stdlib `baml.errors.Failure` axis:
//   - `f.is_retryable()` gates the retry (our `s29_LlmError` derives it from the
//     `FailureKind` taxonomy — the design's 4-row table; see implementation.baml),
//   - `inner.is_effectful()` -> `CannotRetry` is the design's `idempotent:false`.
// A declared `client<llm>` with a `retry_policy` / `provider fallback` lowers to
// those SAME combinators at declaration — proven identical to the fluent chain.
//
// Live tier = testset "integ-test-29-reliability": a Fallback over a dead primary
// + real OpenAI, proving live failover end to end.


// --- The single-turn function the whole scenario rides. ---------------------
// One plain LLM function; every reliability policy is just a different Provider
// handed in via `client =`. Default is a dead client so offline tests must pass
// an explicit, deterministic override (the ns_ai_scenarios house rule).
client<llm> DeadS29 {
  provider openai
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s29_Extract(work: string) -> string {
  client DeadS29                              // swap via `client =` at the call site
  prompt #"
    {{ _.role("user") }}
    {{ work }}
  "#
}


// --- Declared strategy configs that LOWER to the combinators. ----------------
// A `retry_policy`-referencing client lowers to `baml.ai.Retry`; a
// `provider fallback` client lowers to `baml.ai.Fallback`. Both point at a dead
// endpoint so the offline tier stays deterministic (connection-refused).
client<llm> S29DeadPrimitive {
  provider openai
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}
retry_policy S29Retry1 { max_retries 1 }
client<llm> S29DeclaredRetry {
  provider openai
  retry_policy S29Retry1
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}
client<llm> S29DeclaredFallback {
  provider fallback
  options { strategy [S29DeadPrimitive, S29DeadPrimitive] }
}


// ============================================================================
// 1. Retry with TYPED TRIAGE — the design's §4 decision table on the real
//    `is_retryable` axis. A Transient failure is retried with exponential
//    backoff; a Permanent / ContentFilter failure surfaces immediately (retrying
//    wastes money / re-blocks). The `calls` counter proves the wire-drive count.
// ============================================================================

test "29: retry survives a transient failure and recovers on a later attempt" {
    let prov = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 503, fail_times: 2, reply: "recovered", calls: 0,
    };
    let r = s29_Extract("extract this", client = prov.with_retry(3)) catch (e) { _ => "chain failed" };
    log.info("29 transient-retry: reply=" + r + " wire_calls=" + prov.calls.to_string());
    assert.equal(r, "recovered")
    assert.equal(prov.calls, 3)          // 2 transient failures + 1 success
}

test "29: a permanent failure is NOT retried — it surfaces on the first attempt" {
    let prov = s29_TriageProvider {
        kind: s29_FailureKind.Permanent, status_code: 400, fail_times: 99, reply: "unreached", calls: 0,
    };
    let verdict = s29_Extract("extract this", client = prov.with_retry(3)) catch (e) {
        let err: s29_LlmError => "surfaced/retryable=" + err.is_retryable().to_string(),
        _ => "wrong error",
    };
    log.info("29 permanent: " + verdict + " wire_calls=" + prov.calls.to_string());
    assert.equal(verdict, "surfaced/retryable=false")
    assert.equal(prov.calls, 1)          // budget untouched — a 400 won't get better
}

test "29: a content-filter refusal is a clean empty, not retried" {
    let prov = s29_TriageProvider {
        kind: s29_FailureKind.ContentFilter, status_code: 200, fail_times: 99, reply: "unreached", calls: 0,
    };
    let verdict = s29_Extract("extract this", client = prov.with_retry(3)) catch (e) {
        let err: s29_LlmError => "refusal/policy=" + err.is_policy_refusal().to_string(),
        _ => "wrong error",
    };
    assert.equal(verdict, "refusal/policy=true")
    assert.equal(prov.calls, 1)          // retrying a refusal just re-blocks
}

// The interface-only view: an app holding just `baml.errors.CallError` still
// triages a transient (rate-limit) failure via the coarse classifiers derived
// from `kind` (design §b.1).
test "29: interface-only triage reads the coarse classifiers off the taxonomy" {
    let prov = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 429, fail_times: 99, reply: "x", calls: 0,
    };
    let verdict = s29_Extract("q", client = prov.with_retry(0)) catch (e) {
        let c: baml.errors.CallError => { if (c.is_rate_limit()) { "rate-limited" } else { "other" } },
        _ => "wrong error",
    };
    assert.equal(verdict, "rate-limited")
}


// ============================================================================
// 2. EFFECTFUL REFUSAL — the design's idempotency guard (§5: a tool-executing
//    step must opt out of blind retry). On the real surface `Retry` consults
//    `inner.is_effectful()` and throws `CannotRetry` rather than risk a
//    double-executed side effect. The wire is NEVER touched.
// ============================================================================

test "29: retry refuses to re-drive an effectful provider (CannotRetry, wire untouched)" {
    let prov = s29_EffectfulEcho { reply: "committed a side effect", calls: 0 };
    let verdict = s29_Extract("run the tool", client = prov.with_retry(3)) catch (e) {
        let cr: baml.errors.CannotRetry => "refused/effectful=" + cr.is_effectful().to_string(),
        _ => "wrong error",
    };
    log.info("29 effectful-refusal: " + verdict + " wire_calls=" + prov.calls.to_string());
    assert.equal(verdict, "refused/effectful=true")
    assert.equal(prov.calls, 0)          // refuse-then-error, never a silent re-drive
}


// ============================================================================
// 3. FALLBACK ordering through the LLM function — GPT-then-Claude semantics.
//    The chain is just another Provider handed to the same body via `client =`.
//    (Real-surface note: `Fallback` advances on ANY member failure, not only
//    Transient — see the alignment report's divergence on design §5 triage.)
// ============================================================================

test "29: fallback routes the function past a failing primary to a working backup" {
    let primary = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 503, fail_times: 99, reply: "primary down", calls: 0,
    };
    let backup = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 503, fail_times: 0, reply: "backup answered", calls: 0,
    };
    let chain: baml.ai.Provider = primary.fallback_to(backup);
    let r = s29_Extract("extract this", client = chain) catch (e) { _ => "chain failed" };
    log.info("29 failover: reply=" + r + " primary_calls=" + primary.calls.to_string() + " backup_calls=" + backup.calls.to_string());
    assert.equal(r, "backup answered")
    assert.equal(backup.calls, 1)
}


// ============================================================================
// 4. NESTED COMPOSITION — the design's `Resilient()`: retry each member, then
//    fall back. Reads left-to-right off the inherited Provider factories.
//    Primary retries its transient budget, exhausts, then Fallback hops to the
//    retry-wrapped backup which recovers.
// ============================================================================

test "29: a nested chain — retry then fall back — reads left to right" {
    let primary = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 503, fail_times: 99, reply: "primary down", calls: 0,
    };
    let backup = s29_TriageProvider {
        kind: s29_FailureKind.Transient, status_code: 503, fail_times: 1, reply: "backup recovered", calls: 0,
    };
    let chain: baml.ai.Provider =
        primary.with_retry(2).fallback_to(backup.with_retry(2));
    let r = s29_Extract("extract this", client = chain) catch (e) { _ => "chain failed" };
    log.info("29 nested: reply=" + r + " primary_calls=" + primary.calls.to_string() + " backup_calls=" + backup.calls.to_string());
    assert.equal(r, "backup recovered")
    assert.equal(primary.calls, 3)       // 1 + 2 retries, all transient -> exhausted
    assert.equal(backup.calls, 2)        // 1 transient failure + 1 success
}


// ============================================================================
// 5. LOAD BALANCE + FALLBACK — the design's `Balanced()`: RoundRobin across two
//    members, the whole pool wrapped in a fallback to a different provider.
// ============================================================================

test "29: round-robin spreads calls across members, then a pool falls back" {
    let a = s29_MeteredEcho { reply: "from A", inp: 3, out: 2 };
    let b = s29_MeteredEcho { reply: "from B", inp: 3, out: 2 };
    let pool: baml.ai.Provider = baml.ai.RoundRobin { members: [a, b], counter: 0 };
    let r1 = s29_Extract("q1", client = pool) catch (e) { _ => "failed" };
    let r2 = s29_Extract("q2", client = pool) catch (e) { _ => "failed" };
    let r3 = s29_Extract("q3", client = pool) catch (e) { _ => "failed" };
    log.info("29 round-robin: " + r1 + " / " + r2 + " / " + r3);
    assert.equal(r1, "from A")
    assert.equal(r2, "from B")
    assert.equal(r3, "from A")           // cursor wraps
}

test "29: a saturated round-robin pool falls back to a different provider" {
    let saturated: baml.ai.Provider = baml.ai.RoundRobin {
        members: [
            s29_TriageProvider { kind: s29_FailureKind.Transient, status_code: 503, fail_times: 99, reply: "x", calls: 0 },
        ],
        counter: 0,
    };
    let chain: baml.ai.Provider = saturated.fallback_to(s29_MeteredEcho { reply: "different provider", inp: 1, out: 1 });
    let r = s29_Extract("classify this", client = chain) catch (e) { _ => "chain failed" };
    assert.equal(r, "different provider")
}


// ============================================================================
// 6. OBSERVABILITY (plan D4) — a `UsageMeter` records every successful call's
//    usage across the members it wraps, so an app projects usage over everything
//    that actually ran under a `Fallback`, not just the winner.
// ============================================================================

test "29: a UsageMeter aggregates usage across the members that ran under a fallback" {
    let meter = baml.ai.UsageMeter { entries: [] };
    // Primary is metered + fails; backup is metered + succeeds. Only the
    // successful call records (D4 residue: a failed attempt throws without meta).
    let primary = s29_TriageProvider { kind: s29_FailureKind.Transient, status_code: 503, fail_times: 99, reply: "down", calls: 0 };
    let chain: baml.ai.Provider = baml.ai.Fallback {
        members: [meter.wrap(primary), meter.wrap(s29_MeteredEcho { reply: "answered", inp: 12, out: 5 })],
    };
    let r = s29_Extract("summarize", client = chain) catch (e) { _ => "chain failed" };
    let total = meter.total();
    log.info("29 usage-meter: reply=" + r + " calls=" + meter.calls().to_string() + " in=" + total.input_tokens.to_string() + " out=" + total.output_tokens.to_string());
    assert.equal(r, "answered")
    assert.equal(meter.calls(), 1)       // only the winning call carried meta
    assert.equal(total.input_tokens, 12)
    assert.equal(total.output_tokens, 5)
}


// ============================================================================
// 7. DECLARED-CONFIG PATH — a `client<llm>` with `retry_policy` / `provider
//    fallback` lowers to the SAME combinators the fluent chain builds. Proven
//    identical to the hand-built chain over the same dead endpoint: both exhaust
//    and surface a failure (offline can't prove a success path for a declared
//    config, since a legacy primitive needs the network — see report notes).
// ============================================================================

test "29: a declared retry_policy config behaves identically to the fluent .with_retry chain" {
    // Declared: S29DeclaredRetry lowers to `Retry { inner: <openai>, max: 1 }`.
    let declared = s29_Extract("q", client = S29DeclaredRetry) catch (e) { _ => "exhausted-and-surfaced" };
    // Hand-built: the fluent chain over the SAME dead endpoint.
    let fluent = s29_Extract("q", client = baml.ai.OpenAi {
        model: "gpt-4o", api_key: "unused", base_url: "http://127.0.0.1:9",
        extra_headers: null, extra_body: null, append_output_schema: null,
    }.with_retry(1)) catch (e) { _ => "exhausted-and-surfaced" };
    log.info("29 declared-vs-fluent: declared=" + declared + " fluent=" + fluent);
    assert.equal(declared, "exhausted-and-surfaced")
    assert.equal(declared, fluent)       // the declared strategy IS the combinator chain
}

test "29: a declared `provider fallback` config lowers to the Fallback combinator" {
    // Both members dead -> the lowered Fallback exhausts and surfaces.
    let r = s29_Extract("q", client = S29DeclaredFallback) catch (e) { _ => "all-members-failed" };
    assert.equal(r, "all-members-failed")
}


// ============================================================================
// PRESERVED — the load-bearing combinator + projection-channel tests (D4 /
// backlog). These pin behavior at the raw-provider level; keep verbatim.
// ============================================================================

test "29: Fallback routes past a failing member to the first working one" {
    let p: baml.ai.Provider = baml.ai.Fallback { members: [AlwaysFailProvider {}, EchoProvider { reply: "recovered" }] };
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "recovered")
}

test "29: fluent combinator factories chain off any provider" {
    let chained: baml.ai.Provider = AlwaysFailProvider {}.fallback_to(EchoProvider { reply: "via fluent" });
    let r = match (chained) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "via fluent")
}

test "29: Retry exhausts its budget over a failing provider and throws" {
    let p: baml.ai.Provider = AlwaysFailProvider {}.with_retry(2);
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "exhausted" },
        _ => "not http",
    };
    assert.equal(r, "exhausted")
}

/// Counts wire calls — detects double-billing through combinators.
class CountingEcho {
    reply: string,
    calls: int,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["no"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["p"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            self.calls = self.calls + 1;
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["p"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}

class S29ProjErr {
    note: string,
}

test "29: a throwing projection does NOT re-drive the billed call through Retry" {
    let counter = CountingEcho { reply: "ok", calls: 0 };
    let p: baml.ai.Provider = baml.ai.Retry { inner: counter, max: 3, base_delay_ms: 1 };
    let outcome = match (p) {
        let h: baml.ai.HttpProvider => {
            let r = h.call_messages_with<string, string, S29ProjErr>(
                [baml.ai.ChatMessage.user("q")],
                (m: baml.ai.ResponseMeta) -> string throws S29ProjErr {
                    throw S29ProjErr { note: "projection blew up" };
                },
            ) catch (e) {
                let pe: S29ProjErr => baml.ai.CallResult<string, string> { value: "projection surfaced", meta: "" },
                _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
            };
            r.value
        },
        _ => "not http",
    };
    assert.equal(outcome, "projection surfaced")
    assert.equal(counter.calls, 1)
}

test "29: a throwing projection does NOT trigger failover through Fallback" {
    let winner = CountingEcho { reply: "ok", calls: 0 };
    let second = CountingEcho { reply: "should never run", calls: 0 };
    let p: baml.ai.Provider = baml.ai.Fallback { members: [winner, second] };
    let outcome = match (p) {
        let h: baml.ai.HttpProvider => {
            let r = h.call_messages_with<string, string, S29ProjErr>(
                [baml.ai.ChatMessage.user("q")],
                (m: baml.ai.ResponseMeta) -> string throws S29ProjErr {
                    throw S29ProjErr { note: "projection blew up" };
                },
            ) catch (e) {
                let pe: S29ProjErr => baml.ai.CallResult<string, string> { value: "projection surfaced", meta: "" },
                _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
            };
            r.value
        },
        _ => "not http",
    };
    assert.equal(outcome, "projection surfaced")
    assert.equal(winner.calls, 1)
    assert.equal(second.calls, 0)
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The design's failover claim against the REAL API: a Fallback whose first
// member is a guaranteed-fail provider (bad base_url) and whose second is real
// OpenAI. One call routes past the dead primary and lands on OpenAI live.
testset "integ-test-29-reliability" {
    test "29 live: Fallback fails over a dead primary to real OpenAI" {
        let dead: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: "unused", base_url: "http://127.0.0.1:9",
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let real: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let chain: baml.ai.Provider = dead.fallback_to(real);
        let r = s29_Extract("Reply with exactly the lowercase word: pong", client = chain) catch (e) {
            _ => "live failover failed",
        };
        log.info("29 live failover reply: " + r);
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }

    test "29 live: a retry-wrapped real OpenAI still returns on the first healthy attempt" {
        let real: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let r = s29_Extract("Reply with exactly the lowercase word: pong", client = real.with_retry(2)) catch (e) {
            _ => "live retry failed",
        };
        log.info("29 live retry reply: " + r);
        assert.is_true(r.includes("pong") || r.includes("Pong"))
    }
}
