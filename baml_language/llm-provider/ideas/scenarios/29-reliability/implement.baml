// ============================================================================
// 29-reliability / implement.baml  —  STD-LIB / library-author code
//
// Reliability is orchestration that WRAPS a Provider, exactly the spine says:
// "orchestration (retry/fallback/cache) lives in combinators that wrap a
// Provider." The old ClientType enum loop (ns_llm/llm_types.baml:6,273) becomes
// three plain, non-generic combinator classes — Retry, Fallback, RoundRobin —
// that each forward by runtime delegation. They forward request/response, so they
// `implements HttpProvider` (call/call_with) — per provider-as-marker.md, `Provider`
// is a bare marker and `call`/`call_with` live on `HttpProvider`. They are still
// providers (the marker) because `HttpProvider requires Provider`.
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
// 1. The error taxonomy — a concrete CallError rich enough to drive retry.
//
// The canonical channel is `baml.ExtendUnknownError<baml.errors.CallError>`: a
// CallError carries only the three coarse classifiers (is_network_error /
// is_rate_limit / is_parse_error). Those three answer "is this transport noise?"
// but NOT the finer "is this worth retrying, or context-length, or a clean
// content-filter empty?" the background's 4-row table demands. So this scenario
// supplies a CONCRETE CallError — `class LlmError` — that carries a richer
// `FailureKind` discriminator AND implements the canonical classifiers (derived
// from `kind`), so combinators `match` on the concrete `kind` while app code that
// only holds `baml.errors.CallError` still triages via the interface methods.
//
// LlmError ALSO implements StreamError and ToolError: the very same condition
// (a 429, a content-filter) surfaces mid-stream and mid-tool-loop, so the one
// concrete error is legal on all three capability channels.
//
// NET-NEW (flagged in evaluation.md (b)): the `FailureKind` refinement beyond the
// three canonical classifiers. Without it, retry is reduced to string-sniffing
// `e.message` (what the TS background does: `/context length/i.test(err.message)`)
// — exactly the fragility we want to delete along with the provider enum.
// ----------------------------------------------------------------------------

enum FailureKind {
  Transient        // 429, 500/502/503, conn reset, timeout — retry w/ backoff
  Permanent        // 400 bad request, 401/403 auth, 404 model-not-found — never retry
  ContextLength    // "maximum context length exceeded" — needs a DIFFERENT fix
  ContentFilter    // finish_reason: content_filter / safety — a *successful* empty
}

// The concrete domain error every provider throws on the call path. It is a
// CallError (and StreamError / ToolError, since the same condition recurs on those
// channels). The three canonical classifiers are DERIVED from `kind` so the
// interface contract stays consistent with the richer discriminator combinators
// read directly.
class LlmError {
  kind: FailureKind
  status_code: int?                 // HTTP status when transport-level
  message: string
  retry_after_secs: float?          // parsed from the `Retry-After` header (429/503)
  // Surfaced so a context-length handler can decide how much to shed.
  provider_name: string

  implements baml.errors.CallError {
    // Transport-level Transient with no status (conn reset / timeout) is a network
    // error; a 429/503/529 Transient is the rate-limit/overload signal.
    function is_network_error(self) -> bool {
      return self.kind == FailureKind.Transient && self.status_code == null;
    }
    function is_rate_limit(self) -> bool {
      return self.kind == FailureKind.Transient &&
        (self.status_code == 429 || self.status_code == 503 || self.status_code == 529);
    }
    function is_parse_error(self) -> bool {
      // A schema-parse failure is a Permanent 200 in this taxonomy.
      return self.kind == FailureKind.Permanent && self.status_code == 200;
    }
  }
  // Same error can surface mid-stream and mid-tool-loop.
  implements baml.errors.StreamError {
    function is_network_error(self) -> bool {
      return self.kind == FailureKind.Transient && self.status_code == null;
    }
  }
  implements baml.errors.ToolError {}
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

// Minimal typed shapes the classifier coerces the 200-body into, so the
// content-filter signal is read by typed field access rather than the invented
// `Response.json_path` (which does not exist on baml.http.Response). Each provider
// reads only the one field it cares about; extra wire fields are ignored by SAP.
class OpenAIChoice { finish_reason: string? }
class OpenAIBody   { choices: OpenAIChoice[] }
class AnthropicBody { stop_reason: string? }


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
        if (body.includes("maximum context length") ||
            body.includes("context_length_exceeded")) {
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
      let body200 = resp.text() catch (e) { _ => { ""; } };
      let fr = match (baml.json.from_string<OpenAIBody>(body200)) {
        let b: OpenAIBody => match (b.choices.at(0)) {
          let c: OpenAIChoice => (c.finish_reason ?? ""),
          _ => "",
        },
      } catch (e) { _ => { ""; } };
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // service_tier rides as a body field; previous_response_id (if set) threads
      // server-side state. Both are provider-bound, not combinator-bound.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Transport-level failures (DNS, conn reset, timeout) are a CLASSIFIED network
      // error — map them to the concrete LlmError (is_network_error -> true) so the
      // retry loop can read `kind`, rather than routing through UnknownError.
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
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let err = self.classify(from);
      if (err.kind == FailureKind.ContentFilter) { throw err; }   // empty success -> error
      baml.sap.parse<T>(from.text())
        catch (e) { _ => {
          throw LlmError { kind: FailureKind.Permanent, status_code: 200,
            message: "schema parse failed", retry_after_secs: null, provider_name: "openai" };
        } }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
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
        if (body.includes("prompt is too long") ||
            body.includes("context")) {
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
      let body200 = resp.text() catch (e) { _ => { ""; } };
      let sr = match (baml.json.from_string<AnthropicBody>(body200)) {
        let b: AnthropicBody => (b.stop_reason ?? ""),
      } catch (e) { _ => { ""; } };
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic build_request failed");
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
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
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let err = self.classify(from);
      if (err.kind == FailureKind.ContentFilter) { throw err; }
      baml.sap.parse<T>(from.text())
        catch (e) { _ => {
          throw LlmError { kind: FailureKind.Permanent, status_code: 200,
            message: "schema parse failed", retry_after_secs: null, provider_name: "anthropic" };
        } }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
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
// `implements HttpProvider` (it forwards request/response); `with_retry(max)` on the
// Provider marker (the conventions default method) constructs it. The whole point:
// the `match (e.kind)` decides
// transient-retry vs give-up — money and latency live in this match.
// ----------------------------------------------------------------------------

class Retry {
  inner: Provider
  max: int
  base_secs: float          // e.g. 0.5
  cap_secs: float           // e.g. 60.0
  idempotent: bool          // §5 — refuse to retry side-effecting calls

  implements HttpProvider {
    // Retry forwards request/response, so `call`/`call_with` belong to HttpProvider.
    // `call_with` runs the SAME retry loop but threads the caller's `project` through to
    // the inner provider's `call_with` — so usage/sidecar is not dropped by the wrapper.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Only a request/response provider has `call`; match the inner for HttpProvider.
      let h: HttpProvider = match (self.inner) {
        let hh: HttpProvider => hh,
        _ => { throw baml.errors.Unsupported { message: "inner provider has no request/response call" }; },
      };
      let attempt = 0;
      while (true) {
        let r: T = h.call<T>(prompt) catch (e) {
          // A foreign/opaque failure carries no `kind` — we can't prove it's
          // transient, so surface it unchanged rather than guess.
          let u: baml.UnknownError => { throw u; }
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

    // Same retry loop, but route through the inner's call_with so the caller's
    // projection (usage/cost/logprobs) survives the wrapper — the P1 fix. We match
    // the inner for HttpProvider (only a request/response provider has call_with).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.inner) {
        let h: HttpProvider => {
          let attempt = 0;
          while (true) {
            let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) {
              let u: baml.UnknownError => { throw u; }
              let err: LlmError => {
                match (err.kind) {
                  FailureKind.Permanent => { throw err; }
                  FailureKind.ContextLength => { throw err; }
                  FailureKind.ContentFilter => { throw err; }
                  FailureKind.Transient => {
                    if (!self.idempotent && err.status_code == null) { throw err; }
                    if (attempt >= self.max) { throw err; }
                    let wait =
                      if (err.retry_after_secs != null) { err.retry_after_secs }
                      else { jittered_backoff(attempt, self.base_secs, self.cap_secs) };
                    backoff_sleep(wait);
                    attempt = attempt + 1;
                  }
                }
              },
            };
            return r;
          }
          baml.sys.panic("unreachable")
        },
        _ => { throw baml.errors.Unsupported { message: "inner provider has no request/response sidecar" }; },
      }
    }
  }

  // Retry is a pure delivery wrapper — it forwards Streaming/Tools to inner so a
  // retried client is still streamable. (Streaming retry is delicate; see
  // evaluation.md (c).)
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      match (self.inner) {
        let s: Streaming => s.stream<TStream, TFinal>(prompt),
        // Capability negotiation failed — the canonical Unsupported (legal on any
        // capability channel) rather than a hand-rolled "permanent" LlmError.
        _ => { throw baml.errors.Unsupported { message: "inner provider is not streaming" }; },
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

  implements HttpProvider {
    // Fallback forwards request/response, so `call`/`call_with` belong to HttpProvider.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let last: LlmError = LlmError { kind: FailureKind.Permanent, status_code: null,
        message: "no providers", retry_after_secs: null, provider_name: "fallback" };
      for (let p in self.strategy) {
        let h: HttpProvider = match (p) {
          let hh: HttpProvider => hh,
          _ => { continue; },     // non-http member can't serve a value here
        };
        let r: T = h.call<T>(prompt) catch (e) {
          // An opaque failure has no `kind` to decide "would another provider
          // succeed?" — don't launder it across the fallback; surface immediately.
          let u: baml.UnknownError => { throw u; }
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

    // Same fallback walk, forwarding the caller's projection to the chosen member's
    // call_with so usage/sidecar rides through (the P1 fix). A member's AggregateMeta
    // could fold per-attempt cost; here we forward the winner's meta to `project`.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let last: LlmError = LlmError { kind: FailureKind.Permanent, status_code: null,
        message: "no providers", retry_after_secs: null, provider_name: "fallback" };
      for (let p in self.strategy) {
        let h: HttpProvider = match (p) {
          let hh: HttpProvider => hh,
          _ => { continue; },
        };
        let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) {
          let u: baml.UnknownError => { throw u; }
          let err: LlmError => {
            last = err;
            match (err.kind) {
              FailureKind.Transient => { continue; }
              FailureKind.ContextLength => { continue; }
              FailureKind.Permanent => { throw err; }
              FailureKind.ContentFilter => { throw err; }
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
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      for (let p in self.strategy) {
        match (p) {
          let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
          _ => { continue; },
        }
      }
      throw baml.errors.Unsupported { message: "no streaming provider in fallback" }
    }
  }

  // Tools forwarding is where fallback semantics LEAK: a Transcript is provider-
  // owned and opaque (conventions §Tools). A mid-loop transcript from member[0]
  // is meaningless to member[1]. So Fallback can only pick ONE tools member up
  // front and ride it for the whole loop — it cannot fail over mid-transcript.
  implements Tools {
    type Transcript = OpaqueTranscript          // erased wrapper; see net-new flag
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> OpaqueTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      for (let p in self.strategy) {
        match (p) {
          let t: Tools => {
            let inner = t.begin<T>(prompt, tools) catch (e) {
              let u: baml.UnknownError => { throw u; }   // opaque — can't reclassify
              let err: LlmError => {
                match (err.kind) { FailureKind.Transient => { continue; }, _ => { throw err; } }
              }
            };
            return OpaqueTranscript { provider: t, inner_handle: inner };
          },
          _ => { continue; },
        }
      }
      throw baml.errors.Unsupported { message: "no tools provider in fallback" }
    }
    function step<T>(self, t: OpaqueTranscript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Bound to whichever member began. NO failover here — see evaluation.md (c).
      t.provider.step<T>(t.inner_handle)
    }
    function submit(self, t: OpaqueTranscript, results: ToolResult[]) -> OpaqueTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
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

  implements HttpProvider {
    // RoundRobin spreads request/response load, so `call`/`call_with` belong to HttpProvider.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let last: LlmError = LlmError { kind: FailureKind.Transient, status_code: null,
        message: "all members cooling down", retry_after_secs: null, provider_name: "roundrobin" };
      // Try up to `len(members)` healthy picks this call.
      let tries = 0;
      while (tries < self.members.length()) {
        let idx = rr_next_healthy(self.state);          // host: skips cooled-down members
        if (idx < 0) { throw last; }                    // everything is in cooldown
        let h: HttpProvider = match (self.members[idx]) {
          let hh: HttpProvider => hh,
          _ => { tries = tries + 1; continue; },   // non-http member — skip this pick
        };
        let r: T = h.call<T>(prompt) catch (e) {
          let u: baml.UnknownError => { throw u; }   // opaque — not a load signal
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

    // Same load-balancing walk, forwarding the caller's projection to the chosen
    // member's call_with so usage/sidecar is preserved (the P1 fix).
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let last: LlmError = LlmError { kind: FailureKind.Transient, status_code: null,
        message: "all members cooling down", retry_after_secs: null, provider_name: "roundrobin" };
      let tries = 0;
      while (tries < self.members.length()) {
        let idx = rr_next_healthy(self.state);
        if (idx < 0) { throw last; }
        let h: HttpProvider = match (self.members[idx]) {
          let hh: HttpProvider => hh,
          _ => { tries = tries + 1; continue; },
        };
        let r: (T, U) = h.call_with<T, U>(prompt, project) catch (e) {
          let u: baml.UnknownError => { throw u; }
          let err: LlmError => {
            last = err;
            match (err.kind) {
              FailureKind.Transient => {
                rr_record_fail(self.state, idx, self.allowed_fails, self.cooldown_secs);
                tries = tries + 1;
              }
              _ => { throw err; }
            }
          },
        };
        rr_record_success(self.state, idx);
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
                 state: rr_new_state(members.length()) }
  }
}
function rr_new_state(n: int) -> $rust_type { $rust_io_function }
