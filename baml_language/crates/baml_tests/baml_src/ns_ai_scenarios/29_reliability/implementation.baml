// scenario 29 — reliability: ../../../../../llm-provider/ideas/scenarios/29-reliability/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design's implement.baml hand-rolled three combinator classes (Retry,
// Fallback, RoundRobin), a rich error taxonomy (enum FailureKind + class
// LlmError), and Retry-After/backoff host helpers. On the REAL surface the
// combinators are already stdlib (`baml.ai.Retry`/`Fallback`/`RoundRobin`/
// `Traced`, reached fluently via `.with_retry`/`.fallback_to`/`.traced` on the
// `Provider` marker — see `ns_ai/combinators.baml`), and the retry decision
// rides the stdlib `baml.errors.Failure` axis: `Retry` retries iff the thrown
// error `is_retryable()`, and refuses (`CannotRetry`) if `inner.is_effectful()`.
//
// So the only genuinely net-new surface this scenario supplies is the
// PROVIDER-LEVEL ERROR TAXONOMY that DRIVES retryability (design §b.1/§b.2): a
// concrete `CallError` carrying a richer `FailureKind` discriminator than the
// three coarse classifiers, whose `is_retryable()` derivation is exactly the
// design's 4-row decision table (transient retries; permanent / context-length /
// content-filter surface). The offline fakes below let usage.baml exercise
// retry triage, effectful refusal, fallback ordering, and usage metering
// deterministically with no network.


// ---------------------------------------------------------------------------
// The taxonomy (design §b.1): four shapes of failure that the coarse
// is_network/is_rate_limit/is_parse_error trio can't tell apart. Each provider
// classifies its wire dialect into ONE of these; the combinator only ever sees
// the normalized kind via the `Failure` axis.
// ---------------------------------------------------------------------------
enum s29_FailureKind {
    Transient        // 429 / 500 / 503 / 529 / conn-reset / timeout — retry w/ backoff
    Permanent        // 400 bad request, 401/403 auth, 404 not-found — never retry
    ContextLength    // "maximum context length exceeded" — needs a different fix, not a retry
    ContentFilter    // finish_reason content_filter / safety refusal — a clean empty
}

// The concrete domain error a provider throws on the call path. It is a
// `CallError` (and `StreamError`, since the same condition recurs mid-stream),
// and — crucially — a `Failure`: `is_retryable()` is DERIVED from `kind`, so the
// stdlib `Retry` combinator's `f.is_retryable()` check enacts the design's
// decision table with no combinator-side taxonomy. The three coarse CallError
// classifiers are likewise derived from `kind`, so interface-only app code still
// triages correctly (design §b.1: "combinators read the rich kind while
// interface-only callers triage via the classifiers").
class s29_LlmError {
    kind: s29_FailureKind,
    status_code: int?,               // HTTP status when transport-level (429/503/…); null on conn-reset
    message: string,
    provider_name: string,

    implements baml.errors.Failure {
        // THE load-bearing derivation: only a Transient failure is worth
        // retrying. A 400/auth (Permanent), an over-long prompt (ContextLength),
        // and a policy refusal (ContentFilter) all surface immediately — retrying
        // wastes money and re-blocks. This single line is the design's §4 Retry
        // `match (err.kind)` table, expressed on the real `is_retryable` axis.
        function is_retryable(self) -> bool { self.kind == s29_FailureKind.Transient }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { self.kind == s29_FailureKind.ContentFilter }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }

    implements baml.errors.CallError {
        // Transport-level Transient with no status (conn reset / timeout) is a
        // network error; a 429/503/529 Transient is the rate-limit/overload signal.
        function is_network_error(self) -> bool {
            self.kind == s29_FailureKind.Transient && self.status_code == null
        }
        function is_rate_limit(self) -> bool {
            self.kind == s29_FailureKind.Transient &&
                (self.status_code == 429 || self.status_code == 503 || self.status_code == 529)
        }
        function is_parse_error(self) -> bool {
            // A schema-parse failure is a Permanent 200 in this taxonomy.
            self.kind == s29_FailureKind.Permanent && self.status_code == 200
        }
    }

    // The same condition can surface mid-stream.
    implements baml.errors.StreamError {
        function is_network_error(self) -> bool {
            self.kind == s29_FailureKind.Transient && self.status_code == null
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #1 — a provider that fails `fail_times` with a TYPED
// `s29_LlmError` of the given `kind`, counting every wire attempt, then answers
// `reply`. It overrides `call_messages_with` (the primitive from which `call`
// derives), so wrapping it in `Retry`/`Fallback`/`RoundRobin` exercises the real
// combinator loops. `calls` is the observability hook: a test reads it to prove
// how many times the wire was actually driven (double-billing detector).
// ---------------------------------------------------------------------------
class s29_TriageProvider {
    kind: s29_FailureKind,           // the failure kind thrown while failing
    status_code: int?,               // the status the thrown error carries
    fail_times: int,                 // fail this many attempts before succeeding
    reply: string,                   // the eventual success answer
    calls: int,                      // wire-call counter (starts 0)

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["triage: no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["triage: no send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["triage parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            self.calls = self.calls + 1;
            if (self.calls <= self.fail_times) {
                throw s29_LlmError {
                    kind: self.kind,
                    status_code: self.status_code,
                    message: "triage failure #" + self.calls.to_string(),
                    provider_name: "triage",
                };
            }
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["triage parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #2 — an EFFECTFUL provider (design §5: a tool-executing / server-
// stored step whose blind re-drive double-runs side effects). `is_effectful()`
// is the real surface's replacement for the design's `Retry.idempotent: false`
// field: `Retry` consults it and throws `CannotRetry` rather than silently
// re-driving. `calls` proves the wire is NEVER touched when the refusal fires.
// ---------------------------------------------------------------------------
class s29_EffectfulEcho {
    reply: string,
    calls: int,

    implements baml.ai.Provider {
        function is_effectful(self) -> bool throws never { true }
    }

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["effectful: no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["effectful: no send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["effectful parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            self.calls = self.calls + 1;
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["effectful parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// ---------------------------------------------------------------------------
// Offline fake #3 — a metered echo, so the D4 observability combinators
// (`UsageMeter`/`Traced`, the real surface's answer to "project usage over
// everything that actually ran under a Fallback") carry real token numbers
// offline. Reports scripted usage via a ResponseMeta.
// ---------------------------------------------------------------------------
class s29_UsageMeta {
    inp: int,
    out: int,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.inp, output_tokens: self.out }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "n/a" }
        }
    }
}

class s29_MeteredEcho {
    reply: string,
    inp: int,
    out: int,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered: no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered: no send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["metered parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            s29_UsageMeta { inp: self.inp, out: self.out }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["metered parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(s29_UsageMeta { inp: self.inp, out: self.out }) }
        }
    }
}
