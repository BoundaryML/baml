// ============================================================================
// Scenario 07 — Reasoning models (effort, verbosity, thinking, continuity)
// implement.baml — STD-LIB / library-author code.
//
// Builds on the canonical primitives in ../_conventions.md (Provider,
// HttpProvider, Streaming). Net-new surface introduced here is flagged in
// evaluation.md:
//   - `Reasoning` content kind (a distinct value, NOT text)
//   - `effort` (categorical) AND `thinking_budget` (token) provider fields
//   - `verbosity` provider field
//   - `Thinks` capability — extracts reasoning out-of-band from `call`
//   - `Continuity` capability — lossless round-trip of opaque/signed reasoning
//   - `WithReasoning<T>` carrier type so a plain `-> T` function can still
//     surface reasoning + continuity handle alongside the answer
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
// 2. The carrier: a plain `-> T` function still needs reasoning alongside T
// ---------------------------------------------------------------------------
// The answer type the app author writes is `Resume`, `Diagnosis`, etc. — NOT
// "reasoning + Resume". So we let the app keep `-> T` and surface reasoning via
// an explicit wrapper type they opt into: `WithReasoning<T>`. The provider's
// `parse<WithReasoning<T>>` peels the reasoning blocks off the wire and SAP-
// parses the remaining answer into `T`. This is the same trick `GptImage` uses
// (output-specialized parse) — the seam is `parse<T>`, not a new interface.

class WithReasoning<T> {
  answer: T
  reasoning: ReasoningBlock[]      // may be empty (effort=none, or non-reasoning model)
  // Opaque continuity handle to feed into the NEXT turn (see §4). Null when the
  // provider has no cross-turn reasoning state to preserve.
  continue_from: ContinuationState?
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

// Continuity is a capability: not every provider can resume reasoning. A
// provider that implements it promises a lossless `apply` that injects prior
// state into the next prompt's request.
interface Continuity requires Provider {
  // Fold a prior turn's reasoning state into the prompt for the next call.
  // Throws if `state.origin_provider` doesn't match this provider (no cross-
  // provider reasoning continuity — it is genuinely lossy).
  function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
      -> WithReasoning<T> throws Error
}

// `Thinks` is the capability that says "this provider produces a distinct
// reasoning content kind." It refines `call` the way `Streaming` refines
// delivery: the plain `.call<T>` still returns just `T` (reasoning discarded);
// `.think<T>` returns `WithReasoning<T>` so reasoning is surfaced out-of-band.
interface Thinks requires Provider {
  function think<T>(self, prompt: baml.llm.PromptAst) -> WithReasoning<T> throws Error
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
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // $rust_io_function renders prompt -> Responses `input` items, then sets:
      //   reasoning: { effort: <effort>, summary: <summary>,
      //               encrypted_content: <include_encrypted> }
      //   text:      { verbosity: <verbosity>, format: <SAP schema for T> }
      // Note: structured output goes in text.format (one of the four encodings).
      $rust_io_function
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    // Plain `call<T>` DISCARDS reasoning — answer only. This is what
    // `function Foo(args) -> T` gets for free.
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.json_path("output_text"))
    }
  }

  implements Thinks {
    // `think<T>` surfaces reasoning out-of-band. Same HTTP request as `call`
    // (reasoning is in the response either way) — we just don't throw the
    // reasoning items away.
    function think<T>(self, prompt: baml.llm.PromptAst) -> WithReasoning<T> throws baml.errors.LlmClient {
      let resp = self.send(self.build_request<T>(prompt));
      WithReasoning<T> {
        answer: baml.sap.parse<T>(resp.json_path("output_text")),
        reasoning: OpenAIReasoning.extract_reasoning(resp),
        continue_from: OpenAIReasoning.extract_continuation(self, resp),
      }
    }
  }

  implements Continuity {
    function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
        -> WithReasoning<T> throws baml.errors.LlmClient {
      if (state.origin_provider != "openai") {
        throw baml.errors.LlmClient {
          message: "cannot resume " + state.origin_provider + " reasoning on an OpenAI provider"
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
      let resp = self.send(req);
      WithReasoning<T> {
        answer: baml.sap.parse<T>(resp.json_path("output_text")),
        reasoning: OpenAIReasoning.extract_reasoning(resp),
        continue_from: OpenAIReasoning.extract_continuation(self, resp),
      }
    }
  }

  // --- helpers (statics, no self where possible) ---

  function extract_reasoning(resp: baml.http.Response) -> ReasoningBlock[] {
    // $rust_io_function walks Responses `output[]`, collecting items of
    // type "reasoning": pulls `.summary[]` text and `.encrypted_content`, and
    // reads usage.output_tokens_details.reasoning_tokens.
    $rust_io_function
  }

  function extract_continuation(self, resp: baml.http.Response) -> ContinuationState? {
    // If the provider was configured server-side, hold the response id; else
    // package the encrypted items for client-side resend.
    if (self.include_encrypted) {
      ContinuationState {
        openai_previous_response_id: null,
        resend_blocks: OpenAIReasoning.encrypted_blocks(resp),
        origin_provider: "openai",
      }
    } else {
      ContinuationState {
        openai_previous_response_id: resp.json_path("id"),
        resend_blocks: [],
        origin_provider: "openai",
      }
    }
  }

  function encrypted_blocks(resp: baml.http.Response) -> ReasoningRaw[] {
    $rust_io_function   // map each encrypted reasoning item -> ReasoningRaw{ openai_encrypted: ... }
  }

  function build_with_prev(self, prompt: baml.llm.PromptAst, prev_id: string) -> baml.http.Request throws baml.errors.LlmClient {
    $rust_io_function   // build_request, then set previous_response_id = prev_id, drop resent reasoning
  }

  function build_with_encrypted(self, prompt: baml.llm.PromptAst, blocks: ReasoningRaw[]) -> baml.http.Request throws baml.errors.LlmClient {
    $rust_io_function   // build_request, then prepend the encrypted items to `input`
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

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // DIVERGENCE: thinking goes in `thinking: { type: "enabled",
      // budget_tokens: <thinking_budget> }`, and `max_tokens` MUST exceed the
      // budget or the visible answer truncates to nothing. Structured output is
      // forced-tool injection (a different one of the four encodings than OpenAI).
      $rust_io_function
    }

    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }

    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      // Walk content[] for the `text` block (skip `thinking` blocks), SAP-parse it.
      baml.sap.parse<T>(AnthropicReasoning.answer_text(from))
    }
  }

  implements Thinks {
    function think<T>(self, prompt: baml.llm.PromptAst) -> WithReasoning<T> throws baml.errors.LlmClient {
      let resp = self.send(self.build_request<T>(prompt));
      let blocks = AnthropicReasoning.extract_thinking(resp);
      WithReasoning<T> {
        answer: baml.sap.parse<T>(AnthropicReasoning.answer_text(resp)),
        reasoning: blocks,
        // Continuity is the verbatim signed thinking blocks — client holds them.
        continue_from: ContinuationState {
          openai_previous_response_id: null,
          resend_blocks: AnthropicReasoning.raw_of(blocks),
          origin_provider: "anthropic",
        },
      }
    }
  }

  implements Continuity {
    function resume<T>(self, prompt: baml.llm.PromptAst, state: ContinuationState)
        -> WithReasoning<T> throws baml.errors.LlmClient {
      if (state.origin_provider != "anthropic") {
        throw baml.errors.LlmClient {
          message: "cannot resume " + state.origin_provider + " reasoning on an Anthropic provider"
        };
      }
      // CLIENT-authoritative ONLY: re-inject the signed thinking blocks verbatim
      // into the assistant turn. Editing or dropping a signature breaks the turn.
      let req = AnthropicReasoning.build_with_thinking(self, prompt, state.resend_blocks);
      let resp = self.send(req);
      let blocks = AnthropicReasoning.extract_thinking(resp);
      WithReasoning<T> {
        answer: baml.sap.parse<T>(AnthropicReasoning.answer_text(resp)),
        reasoning: blocks,
        continue_from: ContinuationState {
          openai_previous_response_id: null,
          resend_blocks: AnthropicReasoning.raw_of(blocks),
          origin_provider: "anthropic",
        },
      }
    }
  }

  function answer_text(resp: baml.http.Response) -> string throws baml.errors.LlmClient {
    $rust_io_function   // content[] -> concatenated `text` blocks (NOT thinking blocks)
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
    for (let b in blocks) { out = out.append(b.raw); }
    out
  }

  function build_with_thinking(self, prompt: baml.llm.PromptAst, blocks: ReasoningRaw[]) -> baml.http.Request throws baml.errors.LlmClient {
    $rust_io_function   // build_request, then prepend signed thinking blocks to the assistant turn
  }
}

// ---------------------------------------------------------------------------
// 7. A "reasoning-naive" provider for portability tests: GPT-4o has NO thinking.
//    It implements Provider/HttpProvider but NOT Thinks/Continuity. Used to show
//    where `.think` degrades and where `.resume` honestly errors.
// ---------------------------------------------------------------------------
class PlainChat {
  model: string
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }
  // No `implements Thinks`, no `implements Continuity`. The capability set is
  // the type: this provider literally cannot think.
}
