// ============================================================================
// Scenario 07 — Reasoning models (effort, verbosity, thinking, continuity)
// implement.baml — STD-LIB / library-author code.
//
// Builds on the canonical primitives in ../_conventions.md (Provider,
// HttpProvider, Streaming, AND the value+sidecar model — call_with +
// ResponseMeta). Net-new surface introduced here is flagged in evaluation.md:
//   - `Reasoning` content kind (a distinct value, NOT text)
//   - `effort` (categorical) AND `thinking_budget` (token) provider fields
//   - `verbosity` provider field
//   - `ReasoningMeta` — an EXTERNAL-impl extension of `ResponseMeta` carrying
//     the reasoning *text* (displayable/billable) as a PRODUCT sidecar
//   - `Continuity` capability — lossless round-trip of opaque/signed reasoning
//
// PARTITION (per ../value-sidecar-model.md §5): reasoning has TWO halves, and
// they live on TWO different mechanisms:
//   - reasoning *text* (the displayable/billable `ReasoningBlock[]`) ACCOMPANIES
//     the answer → it is a PRODUCT sidecar → `call_with` + `ResponseMeta`. There
//     is NO `Thinks.think -> WithReasoning<T>` companion and NO `WithReasoning<T>`
//     carrier anymore; the bespoke wrapper is gone.
//   - the continuity handle is a STATEFUL round-trip (resend signed/encrypted
//     blocks, or a server-held response id) → it stays its OWN capability
//     (`Continuity`), because the hard part is *state* (P4), not the carrier
//     shape. `(T, U)` cannot express "thread this back into the next turn."
// ============================================================================

// ---------------------------------------------------------------------------
// 1. Reasoning as a DISTINCT content kind (the load-bearing "what's hard")
// ---------------------------------------------------------------------------
// Reasoning is not text. We model it so it can be displayed, billed, stripped,
// or round-tripped without contaminating the answer `T`. The *body* of a
// reasoning block diverges wildly per provider, so we keep it an enum of the
// three real shapes seen in the background:
//
//   - Anthropic: full `thinking` text PLUS a cryptographic `signature`.
//   - OpenAI Responses: a `summary` (human-readable) and/or an `encrypted`
//     opaque blob you cannot read but must pass back.
//   - Gemini: optional "thought" parts (plain text, no signature, droppable).
//
// `ReasoningBlock` is the *displayable/billable* view. `ContinuationState` (§4)
// is the *round-trippable* view — deliberately separate, because the encrypted
// OpenAI case is opaque even though it is continuity-critical.

class ReasoningBlock {
  // What the user can actually read, if anything. Empty for encrypted-only.
  summary: string
  // Token cost of the hidden deliberation, broken out from answer tokens.
  // (OpenAI: output_tokens_details.reasoning_tokens; Anthropic: counted in
  //  output; Gemini: "thoughts" tokens — provider normalizes into this.)
  reasoning_tokens: int
  // The raw provider-shaped payload, kept verbatim so nothing is lost. This is
  // the union of the three real wire shapes; `parse` fills exactly one arm.
  raw: ReasoningRaw
}

class ReasoningRaw {
  // Anthropic extended-thinking: full text + signature (both required on
  // round-trip when tools are involved — dropping/editing breaks the next turn).
  anthropic_thinking: string?
  anthropic_signature: string?
  // OpenAI Responses: opaque encrypted reasoning item you can't read but resend.
  openai_encrypted: string?
  // Gemini: plain "thought" part, no signature, safe to drop.
  gemini_thought: string?
}

// ---------------------------------------------------------------------------
// Response navigation over the REAL `baml.json` surface (NOT an invented
// `Response.json_path` / `json_path_int`). The provider response types below
// read a handful of fields out of the wire JSON at diverging dotted paths; that
// divergence is pure path *data*. We parse the body once (`baml.json.parse`) and
// walk the resulting `json` union — `map<string, json>` arms via `baml.json.field`,
// `json[]` arms via the Array `.at` method. `JsonNav.str/int(body, "a.0.b")`
// returns the string/int at that dotted path, or a default when any hop is missing.
class JsonNav {
  // Walk one segment deeper into a `json` value: a numeric segment indexes the
  // `json[]` arm, any other segment is a `map<string, json>` key.
  function step(node: json, segment: string) -> json throws never {
    match (node) {
      let arr: json[] => {
        if (segment.is_ascii_numeric() && segment != "") {
          let idx = int.parse(segment) catch (e) { _ => { return null; } };
          match (arr.at(idx)) { null => null, let v: json => v }
        } else { null }
      },
      let obj: map<string, json> => baml.json.field(node, segment),
      _ => null,
    }
  }

  // Resolve a dotted path to the `json` leaf at that path (or `null` if any hop
  // is absent). Path segments are the dotted key string split on ".".
  function get(body: string, path: string) -> json throws never {
    let root: json = baml.json.parse(body) catch (e) { _ => { return null; } };
    let cur: json = root;
    for (let seg in path.split(".")) { cur = JsonNav.step(cur, seg); }
    cur
  }

  // The string at `path`, or "" if absent / not a string.
  function str(body: string, path: string) -> string throws never {
    match (JsonNav.get(body, path)) { let s: string => s, _ => "" }
  }

  // The int at `path`, or 0 if absent / not an int.
  function int(body: string, path: string) -> int throws never {
    match (JsonNav.get(body, path)) { let n: int => n, _ => 0 }
  }
}

// ---------------------------------------------------------------------------
// 2. The carrier: reasoning *text* rides the value+sidecar path, NOT a wrapper
// ---------------------------------------------------------------------------
// The answer type the app author writes is `Resume`, `Diagnosis`, etc. — NOT
// "reasoning + Resume". The earlier design bolted on a bespoke `WithReasoning<T>`
// companion wrapper (`Thinks.think -> WithReasoning<T>`); that is exactly the
// per-sidecar parallel-companion mistake the value+sidecar model retires (P1).
//
// Reasoning *text* ACCOMPANIES the answer — value AND metadata — so it is a
// PRODUCT sidecar: it rides `call_with` + `ResponseMeta`. The app keeps `-> T`
// and projects the reasoning out of the normalized meta:
//
//   let (ans, reasoning): (Diagnosis, Supported<ReasoningBlock[]>) =
//     Diagnose.call_with(log, m => match (m) {
//       let rm: ReasoningMeta => rm.reasoning_blocks(),
//       _                     => Unsupported { reason: "no reasoning meta" },
//     });
//
// `reasoning_blocks()` is a NOVEL dimension (richer than the base
// `ResponseMeta.reasoning() -> Supported<string>` summary), so it rides an
// EXTERNAL-impl interface `ReasoningMeta` reached by `match` (the §7 escape
// hatch in value-sidecar-model.md) — the same interface/concrete/external-impl
// pattern as a `Grounded` dimension. Concrete provider response types implement
// it; the framework wrapper forwards it (§3 below).
//
// The continuity handle is DELIBERATELY NOT here: it is a stateful round-trip,
// not a product sidecar (§4). `call_with` hands you the reasoning *text* for
// display/billing; threading it back is `Continuity.resume`.

// `ReasoningMeta` is the response-metadata dimension for reasoning TEXT. It is
// an external-impl extension of `ResponseMeta` (like `Grounded` in §7 of
// value-sidecar-model.md): not every response carries reasoning, so a projector
// reaches it by runtime `match` and falls back to `Unsupported` when absent.
interface ReasoningMeta {
  // The displayable/billable reasoning blocks normalized off the wire. Lazy:
  // a projector that only wants usage never pays to parse these.
  function reasoning_blocks(self) -> Supported<ReasoningBlock[]>
}

// ---------------------------------------------------------------------------
// 3. The two incompatible control models, reconciled
// ---------------------------------------------------------------------------
// `effort` is categorical (OpenAI). `thinking_budget` is a token count
// (Anthropic/Gemini). They are genuinely different knobs. We DO NOT invent a
// fake unified scalar — instead each provider declares the field(s) it actually
// honors as plain fields (the "options dissolve into fields" rule). A shared
// read-interface lets tooling/combinators reason about "is this a reasoning
// call" uniformly without forcing one knob.

enum Effort { none, low, medium, high, xhigh }   // OpenAI gpt-5.5 rungs
enum Verbosity { low, medium, high }             // independent answer-length knob

// Shared read-interface: a reasoning provider exposes whichever knob it honors.
// Both are optional because no provider honors both. A combinator can read this
// without knowing the concrete provider (mirrors ChatModelOptions in §4 proposal).
interface ReasoningOptions {
  effort: Effort?
  thinking_budget: int?
  verbosity: Verbosity?
}

// ---------------------------------------------------------------------------
// 4. Continuity: lossless round-trip is its own capability
// ---------------------------------------------------------------------------
// Two server-vs-client models collide here:
//   - Anthropic / Gemini: continuity is CLIENT-side — you resend the prior
//     reasoning blocks (with signatures) in the next assistant turn.
//   - OpenAI Responses: continuity is SERVER-side — you pass a
//     `previous_response_id` and the server holds the reasoning; OR you resend
//     the encrypted items yourself.
// `ContinuationState` is an opaque, provider-owned union covering both. The app
// never inspects it — it only threads it back in. This is the reasoning analog
// of the `Tools.Transcript` opaque associated type.

class ContinuationState {
  // OpenAI server-authoritative handle. When present, the next request sends
  // `previous_response_id` and omits resent reasoning entirely.
  openai_previous_response_id: string?
  // Client-authoritative path: the verbatim reasoning blocks to resend. Carries
  // Anthropic signatures and OpenAI encrypted blobs untouched.
  resend_blocks: ReasoningRaw[]
  // Which provider produced this — guards against cross-provider continuity,
  // which is lossy/illegal (Anthropic thinking has no Gemini equivalent).
  origin_provider: string
}

// `ContinuationMeta` is the response-metadata dimension for the continuity
// HANDLE (as opposed to the reasoning text). Extracting the handle is metadata
// that accompanies a turn, so a turn surfaces it via `call_with` + a projection,
// exactly like reasoning text — but what you DO with it (thread it into the next
// turn) is the stateful `Continuity` capability, NOT a product sidecar. So the
// extraction rides `ResponseMeta`; the round-trip rides `Continuity.resume`.
interface ContinuationMeta {
  // The opaque continuity handle to feed into the NEXT turn's `resume`. Absent
  // when the provider has no cross-turn reasoning state (effort=none, plain chat).
  function continuation(self) -> Supported<ContinuationState>
}

// Continuity is a capability: not every provider can resume reasoning. A
// provider that implements it promises a lossless `resume` that injects prior
// state into the next prompt's request. This is the STATEFUL half of reasoning —
// deliberately a capability and NOT a `call_with` sidecar, because `(T, U)`
// cannot express "carry this server/client state across turns" (P4).
interface Continuity requires Provider {
  // Fold a prior turn's reasoning state into the prompt for the next call and
  // return just the answer `T`. Throws if `state.origin_provider` doesn't match
  // this provider (no cross-provider reasoning continuity — it is genuinely lossy).
  //
  // The reasoning TEXT and the NEXT continuation handle for this turn are NOT in
  // the return type: like any other turn, `resume` is also surfaced through
  // `resume_with` (its `call_with` analog) so the product sidecars are projected
  // off the same ResponseMeta. The bespoke `WithReasoning<T>` carrier is gone.
  //
  // NET-NEW error channel: `resume` is NOT a stateless-call refinement — it is a
  // cross-turn, server/client-state-bearing protocol (see evaluation.md). So it
  // declares its own capability-error channel, `baml.errors.ContinuityError`,
  // whose classifiers add `is_cross_provider()` (the origin-mismatch failure the
  // call path has no notion of). The interface is introduced below.
  function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
      -> T throws baml.ExtendUnknownError<baml.errors.ContinuityError>

  // The value+sidecar form of `resume`: returns the answer AND a projection of
  // the resulting ResponseMeta (reasoning text + the next continuation handle),
  // so a multi-turn loop threads state forward without a bespoke wrapper.
  function resume_with<T, U>(self, prompt: baml.llm.PromptAst, state: ContinuationState,
                             project: (ResponseMeta) -> U)
      -> (T, U) throws baml.ExtendUnknownError<baml.errors.ContinuityError>
}

// NOTE: there is NO `Thinks` capability anymore. "This provider produces a
// distinct reasoning content kind" is now answered by whether its ResponseMeta
// implements `ReasoningMeta` — discovered by the same runtime `match` a
// projector does, not a separate companion method. `call_with` already surfaces
// the reasoning out-of-band on the `HttpProvider` capability, so combinators that
// forward `call_with` carry it (no silent drop under Fallback) — which the old
// `Thinks.think` companion did NOT get.

// ---------------------------------------------------------------------------
// NET-NEW capability-error interface: continuity (cross-turn reasoning resume).
// Flagged in evaluation.md. Same shape as the core CapErr interfaces, plus one
// classifier the call path has no notion of: `is_cross_provider`, the runtime
// origin-mismatch guard that resume must enforce (Anthropic state on OpenAI).
// ---------------------------------------------------------------------------
interface baml.errors.ContinuityError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
  function is_cross_provider(self) -> bool
}

// A resume() against state from a different provider: lossless continuity is
// impossible (Anthropic signed thinking has no OpenAI equivalent), so this is a
// deliberate, typed domain failure — not a transport error.
class CrossProviderContinuity {
  expected_provider: string
  state_provider: string
  implements baml.errors.ContinuityError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
    function is_cross_provider(self) -> bool { return true; }
  }
}

// ---------------------------------------------------------------------------
// 5. OpenAI reasoning provider (Responses API): effort + verbosity + encrypted
//    continuity + SERVER-side previous_response_id.
// ---------------------------------------------------------------------------
class OpenAIReasoning {
  model: string                          // "gpt-5.5", "o4-mini"
  api_key: string
  effort: Effort                         // none | low | medium | high | xhigh
  verbosity: Verbosity                   // independent of effort
  summary: string                        // "auto" | "concise" | "detailed" | "none"
  // When true we ask the server to return encrypted reasoning items so WE hold
  // continuity (stateless). When false, continuity is server-side via response id.
  include_encrypted: bool

  implements ReasoningOptions {
    effort as effort
    // OpenAI has no token budget — the read-interface field is absent here.
    thinking_budget = null
    verbosity as verbosity
  }

  implements HttpProvider {
    type Body = baml.http.Response

    // The DIVERGENCE lives here. Effort/verbosity/summary all go into the
    // Responses-API `reasoning{}` and `text{}` blocks — a shape no other
    // provider shares.
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // $rust_io_function renders prompt -> Responses `input` items, then sets:
      //   reasoning: { effort: <effort>, summary: <summary>,
      //               encrypted_content: <include_encrypted> }
      //   text:      { verbosity: <verbosity>, format: <SAP schema for T> }
      // Note: structured output goes in text.format (one of the four encodings).
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai send failed");
    }

    // Plain `call<T>` DISCARDS reasoning — answer only. This is what
    // `function Foo(args) -> T` gets for free.
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(JsonNav.str(from.text(), "output_text"))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }

    // The value+sidecar seam: normalize the wire response into a ResponseMeta
    // (lazy). `call_with` derives from one codec pass and projects this; reasoning
    // surfaces here, not via a `think` companion — and because `call_with` is an
    // `HttpProvider` capability Fallback/Retry forward it instead of dropping it.
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAIReasoningResponse { raw: from, include_encrypted: self.include_encrypted }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai meta_of failed");
    }
  }

  implements Continuity {
    // `resume<T> -> T`: the answer only. Reasoning text + the next continuation
    // handle are PRODUCT sidecars projected via `resume_with` below; the bespoke
    // `WithReasoning<T>` carrier is gone.
    function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
        -> T throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
      let resp = OpenAIReasoning.resume_send(self, prompt, state);
      baml.sap.parse<T>(JsonNav.str(resp.text(), "output_text"))
    } catch (e) {
      // CrossProviderContinuity (a known ContinuityError) passes through unchanged;
      // foreign Io/parse failures from send/parse are normalized.
      _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "openai resume failed");
    }

    // value+sidecar form: same request, but project the resulting ResponseMeta so
    // the caller threads reasoning text + the NEXT continuation handle forward.
    function resume_with<T, U>(self, prompt: baml.llm.PromptAst, state: ContinuationState,
                               project: (ResponseMeta) -> U)
        -> (T, U) throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
      let resp = OpenAIReasoning.resume_send(self, prompt, state);
      let meta: ResponseMeta = OpenAIReasoningResponse { raw: resp, include_encrypted: self.include_encrypted };
      (baml.sap.parse<T>(JsonNav.str(resp.text(), "output_text")), project(meta))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "openai resume_with failed");
    }
  }

  // --- helpers (statics, no self where possible) ---

  // The shared resume request/send path (origin guard + the two continuity arms),
  // factored out of resume / resume_with.
  function resume_send(self, prompt: baml.llm.PromptAst, state: ContinuationState) -> baml.http.Response
      throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
    if (state.origin_provider != "openai") {
      throw CrossProviderContinuity {
        expected_provider: "openai",
        state_provider: state.origin_provider,
      };
    }
    // Two continuity paths, decided by which state arm is populated:
    let req: baml.http.Request =
      if (state.openai_previous_response_id != null) {
        // SERVER-authoritative: send previous_response_id, omit resent items.
        OpenAIReasoning.build_with_prev(self, prompt, state.openai_previous_response_id)
      } else {
        // CLIENT-authoritative: re-inject the encrypted reasoning items verbatim.
        OpenAIReasoning.build_with_encrypted(self, prompt, state.resend_blocks)
      };
    self.send(req)
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "openai resume_send failed");
  }

  function extract_reasoning(resp: baml.http.Response) -> ReasoningBlock[] {
    // $rust_io_function walks Responses `output[]`, collecting items of
    // type "reasoning": pulls `.summary[]` text and `.encrypted_content`, and
    // reads usage.output_tokens_details.reasoning_tokens.
    $rust_io_function
  }

  // (The old `extract_continuation` static is gone: that logic now lives on the
  // response type, `OpenAIReasoningResponse.continuation()`, projected via call_with.)

  function encrypted_blocks(resp: baml.http.Response) -> ReasoningRaw[] {
    $rust_io_function   // map each encrypted reasoning item -> ReasoningRaw{ openai_encrypted: ... }
  }

  function build_with_prev(self, prompt: baml.llm.PromptAst, prev_id: string) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
    $rust_io_function   // build_request, then set previous_response_id = prev_id, drop resent reasoning
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "openai build_with_prev failed");
  }

  function build_with_encrypted(self, prompt: baml.llm.PromptAst, blocks: ReasoningRaw[]) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
    $rust_io_function   // build_request, then prepend the encrypted items to `input`
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "openai build_with_encrypted failed");
  }
}

// The OpenAI concrete ResponseMeta: holds the wire response, normalizes LAZILY.
// Implements the base ResponseMeta (usage/finish_reason/...) PLUS the two
// external-impl reasoning dimensions (ReasoningMeta, ContinuationMeta). A
// projector reaches the reasoning dimensions by `match (m) { let rm: ReasoningMeta => ... }`.
class OpenAIReasoningResponse {
  raw: baml.http.Response
  include_encrypted: bool                  // decides server-id vs client-resend continuation
  implements ResponseMeta {
    function usage(self) -> Usage {
      let body = self.raw.text() catch (e) { _ => { "" } };
      Usage { input:  JsonNav.int(body, "usage.input_tokens"),
              output: JsonNav.int(body, "usage.output_tokens") }
    }
    function finish_reason(self) -> string            { JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "status") }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "openai responses" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "openai responses" } }
    // The base `reasoning()` is the plain-text summary view; the richer blocks
    // ride ReasoningMeta below.
    function reasoning(self)     -> Supported<string>     { JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "output_text_reasoning_summary") }
  }
  implements ReasoningMeta {
    function reasoning_blocks(self) -> Supported<ReasoningBlock[]> {
      OpenAIReasoning.extract_reasoning(self.raw)
    }
  }
  implements ContinuationMeta {
    function continuation(self) -> Supported<ContinuationState> {
      // Same logic the old extract_continuation held, now on the response type.
      if (self.include_encrypted) {
        ContinuationState {
          openai_previous_response_id: null,
          resend_blocks: OpenAIReasoning.encrypted_blocks(self.raw),
          origin_provider: "openai",
        }
      } else {
        ContinuationState {
          openai_previous_response_id: JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "id"),
          resend_blocks: [],
          origin_provider: "openai",
        }
      }
    }
  }
}

// ---------------------------------------------------------------------------
// 6. Anthropic extended-thinking provider: TOKEN budget, signed thinking blocks,
//    CLIENT-side continuity (must resend signatures).
// ---------------------------------------------------------------------------
class AnthropicReasoning {
  model: string                          // "claude-sonnet-4-5"
  api_key: string
  max_tokens: int                        // REQUIRED on Anthropic; also caps reasoning
  thinking_budget: int                   // token budget — the OTHER control model
  // No `effort`, no `verbosity`: Anthropic exposes neither. Answer length is
  // left to the prompt. This is the "two-knob model doesn't map cleanly" leak.

  implements ReasoningOptions {
    // Anthropic has no categorical effort and no verbosity field.
    effort = null
    thinking_budget as thinking_budget
    verbosity = null
  }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // DIVERGENCE: thinking goes in `thinking: { type: "enabled",
      // budget_tokens: <thinking_budget> }`, and `max_tokens` MUST exceed the
      // budget or the visible answer truncates to nothing. Structured output is
      // forced-tool injection (a different one of the four encodings than OpenAI).
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic build_request failed");
    }

    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic send failed");
    }

    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Walk content[] for the `text` block (skip `thinking` blocks), SAP-parse it.
      baml.sap.parse<T>(AnthropicReasoning.answer_text(from))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }

    // value+sidecar seam: reasoning text + the (client-held) continuation handle
    // are normalized lazily off this response. `call_with` projects them.
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      AnthropicReasoningResponse { raw: from }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic meta_of failed");
    }
  }

  implements Continuity {
    function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
        -> T throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
      let resp = AnthropicReasoning.resume_send(self, prompt, state);
      baml.sap.parse<T>(AnthropicReasoning.answer_text(resp))
    } catch (e) {
      // CrossProviderContinuity passes through; foreign send/parse failures normalize.
      _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "anthropic resume failed");
    }

    function resume_with<T, U>(self, prompt: baml.llm.PromptAst, state: ContinuationState,
                               project: (ResponseMeta) -> U)
        -> (T, U) throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
      let resp = AnthropicReasoning.resume_send(self, prompt, state);
      let meta: ResponseMeta = AnthropicReasoningResponse { raw: resp };
      (baml.sap.parse<T>(AnthropicReasoning.answer_text(resp)), project(meta))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "anthropic resume_with failed");
    }
  }

  // Shared resume request/send path (origin guard + client-authoritative resend).
  function resume_send(self, prompt: baml.llm.PromptAst, state: ContinuationState) -> baml.http.Response
      throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
    if (state.origin_provider != "anthropic") {
      throw CrossProviderContinuity {
        expected_provider: "anthropic",
        state_provider: state.origin_provider,
      };
    }
    // CLIENT-authoritative ONLY: re-inject the signed thinking blocks verbatim
    // into the assistant turn. Editing or dropping a signature breaks the turn.
    let req = AnthropicReasoning.build_with_thinking(self, prompt, state.resend_blocks);
    self.send(req)
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "anthropic resume_send failed");
  }

  function answer_text(resp: baml.http.Response) -> string
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    $rust_io_function   // content[] -> concatenated `text` blocks (NOT thinking blocks)
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic answer_text failed");
  }

  function extract_thinking(resp: baml.http.Response) -> ReasoningBlock[] {
    // $rust_io_function: each `thinking` block -> ReasoningBlock{
    //   summary: block.thinking (full text — Anthropic exposes it),
    //   reasoning_tokens: <from usage>,
    //   raw: ReasoningRaw{ anthropic_thinking: block.thinking,
    //                      anthropic_signature: block.signature } }
    $rust_io_function
  }

  function raw_of(blocks: ReasoningBlock[]) -> ReasoningRaw[] {
    let out: ReasoningRaw[] = [];
    for (let b in blocks) { out.push(b.raw); }
    out
  }

  function build_with_thinking(self, prompt: baml.llm.PromptAst, blocks: ReasoningRaw[]) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.ContinuityError> {
    $rust_io_function   // build_request, then prepend signed thinking blocks to the assistant turn
  } catch (e) {
    _ => throw baml.UnknownError.with_message<baml.errors.ContinuityError>(e, "anthropic build_with_thinking failed");
  }
}

// The Anthropic concrete ResponseMeta. Reasoning blocks carry signatures and the
// continuation is the verbatim signed blocks (client-held) — both normalized lazily.
class AnthropicReasoningResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let body = self.raw.text() catch (e) { _ => { "" } };
      Usage { input:  JsonNav.int(body, "usage.input_tokens"),
              output: JsonNav.int(body, "usage.output_tokens") }
    }
    function finish_reason(self) -> string            { JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "stop_reason") }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "anthropic: none" } }
    function citations(self)     -> Supported<Citation[]> { parse_citations(JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "content")) }
    // Anthropic exposes the full thinking text — the base summary view is it.
    function reasoning(self)     -> Supported<string>     { JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "content.thinking") }
  }
  implements ReasoningMeta {
    function reasoning_blocks(self) -> Supported<ReasoningBlock[]> {
      AnthropicReasoning.extract_thinking(self.raw)
    }
  }
  implements ContinuationMeta {
    function continuation(self) -> Supported<ContinuationState> {
      // Continuity is the verbatim signed thinking blocks — client holds them.
      ContinuationState {
        openai_previous_response_id: null,
        resend_blocks: AnthropicReasoning.raw_of(AnthropicReasoning.extract_thinking(self.raw)),
        origin_provider: "anthropic",
      }
    }
  }
}

// ---------------------------------------------------------------------------
// 7. A "reasoning-naive" provider for portability tests: GPT-4o has NO thinking.
//    It implements Provider/HttpProvider but its ResponseMeta does NOT implement
//    ReasoningMeta/ContinuationMeta, and it does NOT implement Continuity. Used to
//    show where the reasoning projection lands on `Unsupported` (via a `_` arm)
//    and where `.resume` honestly errors.
// ---------------------------------------------------------------------------
class PlainChat {
  model: string
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "plainchat build_request failed");
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request)
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "plainchat send failed");
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "plainchat parse failed");
    }
    // Its ResponseMeta is a plain chat response: usage only, and NO ReasoningMeta /
    // ContinuationMeta impls — so a reasoning projection's `match` misses and lands
    // on the projector's `_` arm (`Unsupported`). This is the value+sidecar form of
    // "this provider literally cannot think."
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      PlainChatResponse { raw: from }
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "plainchat meta_of failed");
    }
  }
  // No `implements Continuity`, and its ResponseMeta carries no reasoning
  // dimension. The capability set is the type: this provider literally cannot think.
}

// A plain (non-reasoning) chat response: usage/finish_reason only. It does NOT
// implement ReasoningMeta or ContinuationMeta, so `match (m) { let rm: ReasoningMeta => ... }`
// falls through to the projector's `_` arm — typed "can't" by absence of the impl.
class PlainChatResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let body = self.raw.text() catch (e) { _ => { "" } };
      Usage { input:  JsonNav.int(body, "usage.prompt_tokens"),
              output: JsonNav.int(body, "usage.completion_tokens") }
    }
    function finish_reason(self) -> string            { JsonNav.str(self.raw.text() catch (e) { _ => { "" } }, "choices.0.finish_reason") }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "plain chat" } }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "plain chat" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "non-reasoning model" } }
  }
}
