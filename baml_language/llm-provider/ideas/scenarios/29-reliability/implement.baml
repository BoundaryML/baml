// ============================================================================
// 29-reliability / implement.baml  —  STD-LIB / library-author code
//
// Reliability is orchestration that WRAPS a Provider, exactly the spine says:
// "orchestration (retry/fallback/cache) lives in combinators that wrap a
// Provider." The old ClientType enum loop (ns_llm/llm_types.baml:6,273) becomes
// three plain, non-generic combinator classes — Retry, Fallback, RoundRobin —
// that each `implements Provider` and forward by runtime delegation.
//
// The hard part the background flags ("What's hard"):
//   - retry must read the TYPED throws channel to decide transient vs permanent
//     vs context-length vs content-filter — misclassifying wastes money or
//     gives up too early;
//   - Retry-After must be HONORED, not guessed;
//   - service_tier is a provider FIELD (it dissolves into the provider), not a
//     combinator field;
//   - idempotency: a retry of a non-idempotent step can double-run side effects;
//   - fallback that preserves semantics, and stateful chains that BREAK under
//     fallback (previous_response_id is meaningless to the other provider).
//
// Builds ON the canonical primitives in _conventions.md. Net-new surface is
// flagged in evaluation.md.
// ============================================================================


// ----------------------------------------------------------------------------
// 1. The error taxonomy — a typed throws channel rich enough to drive retry.
//
// The spine's base says `type Error = baml.errors.LlmClient`. That single opaque
// error can't answer "is this worth retrying?". The background's table (Transient
// / Permanent / Context-length / Content-filter) is the contract a retry loop
// needs, so we REFINE the error into a tagged class that any provider throws and
// any combinator can `match` on.
//
// NET-NEW (flagged in evaluation.md (b)): `baml.errors.LlmClient` gains a
// `kind` discriminator + a `retry_after` channel. Without a typed kind, retry is
// reduced to string-sniffing `e.message` (what the TS background does:
// `/context length/i.test(err.message)`) — which is exactly the fragility we want
// to delete along with the provider enum.
// ----------------------------------------------------------------------------

enum FailureKind {
  Transient        // 429, 500/502/503, conn reset, timeout — retry w/ backoff
  Permanent        // 400 bad request, 401/403 auth, 404 model-not-found — never retry
  ContextLength    // "maximum context length exceeded" — needs a DIFFERENT fix
  ContentFilter    // finish_reason: content_filter / safety — a *successful* empty
}

// The shape every provider throws. We treat it as a refinement of the spine's
// `baml.errors.LlmClient` (assume the host error carries these fields, the way
// the OpenAI/Anthropic SDKs expose `.status_code` and `.response.headers`).
class LlmError {
  kind: FailureKind
  status_code: int?                 // HTTP status when transport-level
  message: string
  retry_after_secs: float?          // parsed from the `Retry-After` header (429/503)
  // Surfaced so a context-length handler can decide how much to shed.
  provider_name: string
}

// Classify a raw transport result into the taxonomy. This is the one place the
// four-shapes-of-the-same-condition problem from the background lives
// ("openai.RateLimitError / anthropic.RateLimitError / Gemini ResourceExhausted /
// APICallError.statusCode === 429 — same condition, four shapes"). Each provider
// owns its own `classify` because only it knows its wire dialect; the combinator
// only ever sees the normalized `LlmError`.
interface Classifier {
  function classify(self, resp: baml.http.Response) -> LlmError throws never
}


// ----------------------------------------------------------------------------
// 2. service_tier dissolves into the provider's fields (spine rule §4).
//
// OpenAI: service_tier ∈ "auto" | "default" | "flex" | "priority"
// Anthropic: service_tier ∈ "auto" | "standard_only"  (opt into Priority Tier)
// These are NOT the same knob with the same values — so they are bespoke fields
// on each provider class, NOT a shared blob and NOT a combinator field.
// ----------------------------------------------------------------------------

class OpenAIChat {
  model: string
  api_key: string
  service_tier: string?             // "flex" (cheap/slow) | "priority" (fast/$$) | null
  // previous_response_id is a STATEFUL handle — see §6 (breaks under fallback).
  previous_response_id: string?

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Classifier {
    function classify(self, resp: baml.http.Response) -> LlmError throws never {
      let ra = parse_retry_after(resp);                 // Retry-After honoring
      if (resp.status_code == 429 || resp.status_code == 503) {
        return LlmError {
          kind: FailureKind.Transient, status_code: resp.status_code,
          message: "rate limited / unavailable", retry_after_secs: ra,
          provider_name: "openai",
        };
      }
      if (resp.status_code >= 500) {
        return LlmError {
          kind: FailureKind.Transient, status_code: resp.status_code,
          message: "server error", retry_after_secs: null, provider_name: "openai",
        };
      }
      if (resp.status_code == 400) {
        // context-length is a 400 SUBTYPE — must be split out of Permanent, or a
        // retry loop wastes a fallback hop on an error the next model also rejects.
        let body = resp.text() catch (e) { _ => { ""; } };
        if (baml.str.contains(body, "maximum context length") ||
            baml.str.contains(body, "context_length_exceeded")) {
          return LlmError {
            kind: FailureKind.ContextLength, status_code: 400,
            message: "context length exceeded", retry_after_secs: null,
            provider_name: "openai",
          };
        }
        return LlmError {
          kind: FailureKind.Permanent, status_code: 400,
          message: "bad request", retry_after_secs: null, provider_name: "openai",
        };
      }
      if (resp.status_code == 401 || resp.status_code == 403 || resp.status_code == 404) {
        return LlmError {
          kind: FailureKind.Permanent, status_code: resp.status_code,
          message: "auth / not found", retry_after_secs: null, provider_name: "openai",
        };
      }
      // 200 path: a content_filter finish_reason is a SUCCESSFUL empty, not a
      // transport error — the background's subtle row. It surfaces here so the
      // parse step can refuse to return junk.
      let fr = resp.json_path("choices.0.finish_reason") catch (e) { _ => { ""; } };
      if (fr == "content_filter") {
        return LlmError {
          kind: FailureKind.ContentFilter, status_code: 200,
          message: "blocked by content filter", retry_after_secs: null,
          provider_name: "openai",
        };
      }
      // Not actually an error (200 / end of stream). Caller checks .kind upstream.
      return LlmError {
        kind: FailureKind.Permanent, status_code: resp.status_code,
        message: "unclassified", retry_after_secs: null, provider_name: "openai",
      };
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws LlmError {
      // service_tier rides as a body field; previous_response_id (if set) threads
      // server-side state. Both are provider-bound, not combinator-bound.
      $rust_io_function
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws LlmError {
      // Transport-level failures (DNS, conn reset, timeout) are Transient.
      let resp = baml.http.send(request) catch (e) {
        baml.errors.Timeout => {
          throw LlmError { kind: FailureKind.Transient, status_code: null,
            message: "timeout", retry_after_secs: null, provider_name: "openai" };
        },
        baml.errors.Io => {
          throw LlmError { kind: FailureKind.Transient, status_code: null,
            message: "connection reset", retry_after_secs: null, provider_name: "openai" };
        },
      };
      if (!resp.ok()) { throw self.classify(resp); }     // 4xx/5xx -> typed kind
      return resp;
    }

    function parse<T>(self, from: baml.http.Response) -> T throws LlmError {
      let err = self.classify(from);
      if (err.kind == FailureKind.ContentFilter) { throw err; }   // empty success -> error
      baml.sap.parse<T>(from.text())
        catch (e) { _ => {
          throw LlmError { kind: FailureKind.Permanent, status_code: 200,
            message: "schema parse failed", retry_after_secs: null, provider_name: "openai" };
        } }
    }
  }
}

class Anthropic {
  model: string
  api_key: string
  max_tokens: int
  service_tier: string?             // "auto" (opt into Priority, fall back to std) | "standard_only"

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Classifier {
    function classify(self, resp: baml.http.Response) -> LlmError throws never {
      let ra = parse_retry_after(resp);
      if (resp.status_code == 429 || resp.status_code == 529) {   // 529 = overloaded
        return LlmError { kind: FailureKind.Transient, status_code: resp.status_code,
          message: "rate limited / overloaded", retry_after_secs: ra, provider_name: "anthropic" };
      }
      if (resp.status_code >= 500) {
        return LlmError { kind: FailureKind.Transient, status_code: resp.status_code,
          message: "server error", retry_after_secs: null, provider_name: "anthropic" };
      }
      if (resp.status_code == 400) {
        let body = resp.text() catch (e) { _ => { ""; } };
        if (baml.str.contains(body, "prompt is too long") ||
            baml.str.contains(body, "context")) {
          return LlmError { kind: FailureKind.ContextLength, status_code: 400,
            message: "prompt too long", retry_after_secs: null, provider_name: "anthropic" };
        }
        return LlmError { kind: FailureKind.Permanent, status_code: 400,
          message: "invalid request", retry_after_secs: null, provider_name: "anthropic" };
      }
      if (resp.status_code == 401 || resp.status_code == 403 || resp.status_code == 404) {
        return LlmError { kind: FailureKind.Permanent, status_code: resp.status_code,
          message: "auth / not found", retry_after_secs: null, provider_name: "anthropic" };
      }
      // Anthropic content-filter shape differs: stop_reason + a separate safety
      // signal, NOT a finish_reason string. Provider-local knowledge stays here.
      let sr = resp.json_path("stop_reason") catch (e) { _ => { ""; } };
      if (sr == "refusal") {
        return LlmError { kind: FailureKind.ContentFilter, status_code: 200,
          message: "refusal", retry_after_secs: null, provider_name: "anthropic" };
      }
      return LlmError { kind: FailureKind.Permanent, status_code: resp.status_code,
        message: "unclassified", retry_after_secs: null, provider_name: "anthropic" };
    }
  }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws LlmError {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws LlmError {
      let resp = baml.http.send(request) catch (e) {
        baml.errors.Timeout => {
          throw LlmError { kind: FailureKind.Transient, status_code: null,
            message: "timeout", retry_after_secs: null, provider_name: "anthropic" };
        },
        baml.errors.Io => {
          throw LlmError { kind: FailureKind.Transient, status_code: null,
            message: "io", retry_after_secs: null, provider_name: "anthropic" };
        },
      };
      if (!resp.ok()) { throw self.classify(resp); }
      return resp;
    }
    function parse<T>(self, from: baml.http.Response) -> T throws LlmError {
      let err = self.classify(from);
      if (err.kind == FailureKind.ContentFilter) { throw err; }
      baml.sap.parse<T>(from.text())
        catch (e) { _ => {
          throw LlmError { kind: FailureKind.Permanent, status_code: 200,
            message: "schema parse failed", retry_after_secs: null, provider_name: "anthropic" };
        } }
    }
  }
}


// ----------------------------------------------------------------------------
// 3. Host helpers (genuine boundaries only).
// ----------------------------------------------------------------------------

// Reads `Retry-After` (seconds, or an HTTP-date) plus the advisory
// `x-ratelimit-reset` / `anthropic-ratelimit-*` headers. Returns null if absent,
// so the loop falls back to computed backoff.
function parse_retry_after(resp: baml.http.Response) -> float? {
  $rust_io_function
}

// Sleeps. The only real side effect in a retry loop.
function backoff_sleep(secs: float) -> null {
  $rust_io_function
}

// Decorrelated-jitter exponential backoff: min(cap, rand(base, prev*3)).
function jittered_backoff(attempt: int, base: float, cap: float) -> float {
  $rust_io_function
}


// ----------------------------------------------------------------------------
// 4. Retry — the typed throws channel drives the decision.
//
// Replaces the ClientType retry arm. It is a plain, non-generic class that
// `implements Provider`; `with_retry(max)` on the Provider base (the conventions
// default method) constructs it. The whole point: the `match (e.kind)` decides
// transient-retry vs give-up — money and latency live in this match.
// ----------------------------------------------------------------------------

class Retry {
  inner: Provider
  max: int
  base_secs: float          // e.g. 0.5
  cap_secs: float           // e.g. 60.0
  idempotent: bool          // §5 — refuse to retry side-effecting calls

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws LlmError {
      let attempt = 0;
      while (true) {
        let r: T = self.inner.call<T>(prompt) catch (e) {
          let err: LlmError => {
            // The decision table, in one place:
            match (err.kind) {
              FailureKind.Permanent => { throw err; }       // 400/401/403/404 — never helps
              FailureKind.ContextLength => { throw err; }   // needs prompt-shrink, not a retry
              FailureKind.ContentFilter => { throw err; }   // a clean empty; retrying re-blocks
              FailureKind.Transient => {
                // Idempotency guard: a transient failure AFTER the request may
                // have reached the server is not safe to blindly replay if the
                // call has side effects (the background's tool-executing-step
                // double-run). For a pure completion it's fine.
                if (!self.idempotent && err.status_code == null) {
                  // We never got a response — can't prove the side effect didn't
                  // happen. Surface rather than risk double execution.
                  throw err;
                }
                if (attempt >= self.max) { throw err; }     // budget exhausted
                // HONOR Retry-After if present, else computed jitter.
                let wait =
                  if (err.retry_after_secs != null) { err.retry_after_secs }
                  else { jittered_backoff(attempt, self.base_secs, self.cap_secs) };
                backoff_sleep(wait);
                attempt = attempt + 1;
                // `continue` re-enters the loop -> retries inner.call.
              }
            }
            // After a Transient handled above, fall through to retry by NOT
            // returning; the match arm completed, loop continues.
            // (BAML: the catch block yields control back to the while.)
          },
        };
        return r;   // success
      }
      baml.sys.panic("unreachable")
    }
  }

  // Retry is a pure delivery wrapper — it forwards Streaming/Tools to inner so a
  // retried client is still streamable. (Streaming retry is delicate; see
  // evaluation.md (c).)
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws LlmError {
      match (self.inner) {
        let s: Streaming => s.stream<TStream, TFinal>(prompt),
        _ => { throw LlmError { kind: FailureKind.Permanent, status_code: null,
          message: "inner provider is not streaming", retry_after_secs: null,
          provider_name: "retry" }; },
      }
    }
  }
}


// ----------------------------------------------------------------------------
// 5. Fallback — try each member in order, but only fall through on errors where
//    "a different provider might succeed."
//
// Background: "A fallback that changes behavior silently is worse than a clean
// failure." So Fallback ONLY advances on Transient (the current provider is
// down) and ContextLength (route to a bigger-context model is meaningful). On
// Permanent it surfaces — a malformed schema won't validate anywhere — and on
// ContentFilter it surfaces, because re-asking a second model to produce blocked
// content is not a reliability win, it's circumvention.
// ----------------------------------------------------------------------------

class Fallback {
  strategy: Provider[]

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws LlmError {
      let last: LlmError = LlmError { kind: FailureKind.Permanent, status_code: null,
        message: "no providers", retry_after_secs: null, provider_name: "fallback" };
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) {
          let err: LlmError => {
            last = err;
            match (err.kind) {
              FailureKind.Transient => { continue; }      // this one's down — try next
              FailureKind.ContextLength => { continue; }  // next may have bigger window
              FailureKind.Permanent => { throw err; }     // won't get better elsewhere
              FailureKind.ContentFilter => { throw err; }  // clean refusal — don't launder
            }
          },
        };
        return r;
      }
      throw last
    }
  }

  // Forwards streaming to the first streaming-capable member (conventions
  // pattern). Each implements block is a runtime `match`-and-route.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws LlmError {
      for (let p in self.strategy) {
        match (p) {
          let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
          _ => { continue; },
        }
      }
      throw LlmError { kind: FailureKind.Permanent, status_code: null,
        message: "no streaming provider in fallback", retry_after_secs: null,
        provider_name: "fallback" }
    }
  }

  // Tools forwarding is where fallback semantics LEAK: a Transcript is provider-
  // owned and opaque (conventions §Tools). A mid-loop transcript from member[0]
  // is meaningless to member[1]. So Fallback can only pick ONE tools member up
  // front and ride it for the whole loop — it cannot fail over mid-transcript.
  implements Tools {
    type Transcript = OpaqueTranscript          // erased wrapper; see net-new flag
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> OpaqueTranscript throws LlmError {
      for (let p in self.strategy) {
        match (p) {
          let t: Tools => {
            let inner = t.begin<T>(prompt, tools) catch (e) { let err: LlmError => {
              match (err.kind) { FailureKind.Transient => { continue; }, _ => { throw err; } }
            }};
            return OpaqueTranscript { provider: t, inner_handle: inner };
          },
          _ => { continue; },
        }
      }
      throw LlmError { kind: FailureKind.Permanent, status_code: null,
        message: "no tools provider in fallback", retry_after_secs: null, provider_name: "fallback" }
    }
    function step<T>(self, t: OpaqueTranscript) -> T | ToolCalls throws LlmError {
      // Bound to whichever member began. NO failover here — see evaluation.md (c).
      t.provider.step<T>(t.inner_handle)
    }
    function submit(self, t: OpaqueTranscript, results: ToolResult[]) -> OpaqueTranscript throws LlmError {
      OpaqueTranscript { provider: t.provider, inner_handle: t.provider.submit(t.inner_handle, results) }
    }
  }
}

// NET-NEW: a wrapper so a combinator can carry "which member owns this loop"
// alongside the member's own opaque transcript. The base `Transcript` is an
// associated type per provider; a combinator that forwards Tools needs to box it.
class OpaqueTranscript {
  provider: Tools
  inner_handle: unknown       // the member's own Transcript, type-erased
}


// ----------------------------------------------------------------------------
// 6. RoundRobin — spread load across keys/providers (LiteLLM Router's
//    routing_strategy), with a circuit breaker (allowed_fails / cooldown).
//
// Stateless selection would be trivial, but "least-busy" / cooldown need MUTABLE
// state across calls — which a pure provider function does not have. This is the
// scenario's sharpest stress on the model: see net-new flag + evaluation.md (c).
// ----------------------------------------------------------------------------

class RoundRobin {
  members: Provider[]
  allowed_fails: int          // cool a member down after N consecutive fails
  cooldown_secs: float
  // The cursor + per-member health live in a host-backed cell, because a
  // `client Name()` -> Provider rewrite produces a *fresh value per call* and
  // cannot hold a counter. NET-NEW: a mutable host handle.
  state: $rust_type           // RoundRobinState: cursor + health map + clock

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws LlmError {
      let last: LlmError = LlmError { kind: FailureKind.Transient, status_code: null,
        message: "all members cooling down", retry_after_secs: null, provider_name: "roundrobin" };
      // Try up to `len(members)` healthy picks this call.
      let tries = 0;
      while (tries < baml.array.len(self.members)) {
        let idx = rr_next_healthy(self.state);          // host: skips cooled-down members
        if (idx < 0) { throw last; }                    // everything is in cooldown
        let p = self.members[idx];
        let r: T = p.call<T>(prompt) catch (e) {
          let err: LlmError => {
            last = err;
            match (err.kind) {
              FailureKind.Transient => {
                rr_record_fail(self.state, idx, self.allowed_fails, self.cooldown_secs);
                tries = tries + 1;
                // continue -> next healthy member
              }
              _ => { throw err; }   // permanent/context/filter — not a load problem
            }
          },
        };
        rr_record_success(self.state, idx);             // reset the member's fail count
        return r;
      }
      throw last
    }
  }
}

// Host-backed cursor + circuit-breaker state. NET-NEW host primitive.
function rr_next_healthy(state: $rust_type) -> int { $rust_io_function }      // -1 if none
function rr_record_fail(state: $rust_type, idx: int, allowed: int, cooldown: float) -> null { $rust_io_function }
function rr_record_success(state: $rust_type, idx: int) -> null { $rust_io_function }

// Constructor for the stateful combinator (statics, conventions §6).
class RoundRobin {
  function over(members: Provider[]) -> RoundRobin {
    RoundRobin { members: members, allowed_fails: 2, cooldown_secs: 60.0,
                 state: rr_new_state(baml.array.len(members)) }
  }
}
function rr_new_state(n: int) -> $rust_type { $rust_io_function }
