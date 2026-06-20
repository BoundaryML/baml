// =============================================================================
// Observability — spans, cost, wire-vs-UI message
//
// STD-LIB / library-author code.
//
// The teaching points:
//   1. A `Traced` combinator wraps any Provider and opens/closes a span per
//      `.call`, forwarding every capability (call / stream / tools / realtime).
//   2. Streaming spans: open on first byte, close on last, usage known only at
//      the end. We show the dangling-span hazard handled explicitly.
//   3. Agent-loop nesting: the tool loop runs under one parent span, with a
//      child span per model turn and per tool dispatch. This rides on
//      ExecutionContext.emit (the event surface flagged net-new in §7).
//   4. Usage normalization: every provider reports tokens under different keys;
//      a `Usage` read-interface normalizes them for attribution.
//   5. Wire-message (lean, what the model saw) vs UI-message (rich, persisted)
//      is a real type split, with a redaction pass before export.
//
// Build ON the canonical primitives (Provider / HttpProvider / Streaming /
// Tools / ExecutionContext). Everything below is net-new and flagged in
// evaluation.md.
// =============================================================================

// -----------------------------------------------------------------------------
// Net-new host primitives: a span sink + a clock + a redactor.
// These are the genuine host boundaries (the OTel/Langfuse exporter, wall clock,
// and a PII scrubber are all native).
// -----------------------------------------------------------------------------

class SpanId   { _raw: $rust_type }                 // opaque correlation handle
class Instant  { _raw: $rust_type }                 // monotonic timestamp

// baml.obs (net-new namespace) — the exporter seam.
//   start  -> opens a span under an optional parent, returns its id
//   finish -> closes a span, attaching final attributes (usage, status)
//   event  -> a point-in-time annotation on an open span (e.g. first token)
function obs_start(name: string, parent: SpanId?, attrs: map<string, string>) -> SpanId {
  $rust_io_function
}
function obs_finish(span: SpanId, attrs: map<string, string>, error: string?) -> null {
  $rust_io_function
}
function obs_event(span: SpanId, name: string, attrs: map<string, string>) -> null {
  $rust_io_function
}
function obs_now() -> Instant { $rust_io_function }

// The redactor is host-backed: regex/entity PII scrubbing is not pure BAML.
function redact(text: string) -> string { $rust_io_function }

// -----------------------------------------------------------------------------
// Usage normalization — rides the value+sidecar model (call_with + ResponseMeta).
//
// Every provider buries token counts under different keys (OpenAI:
// usage.prompt_tokens / completion_tokens; Anthropic: usage.input_tokens /
// output_tokens + cache_creation_input_tokens / cache_read_input_tokens). A
// span needs ONE shape to attribute cost. Usage is a PRODUCT sidecar — it
// accompanies the answer — so it is NOT a bespoke `last_usage()` companion
// returning a stateful side-channel. It is a dimension of the normalized
// `ResponseMeta` the provider hands back from `call_with`, read by projection.
// This is the P1 fix applied: the usage travels WITH the value out of one codec
// pass (`meta_of(body)`), so a Fallback forwards it instead of racing a stash.
// -----------------------------------------------------------------------------

class Usage {
  input_tokens: int
  output_tokens: int
  cache_read_tokens: int      // 0 when the provider can't report it
  cache_write_tokens: int     // 0 when the provider can't report it
  reasoning_tokens: int       // 0 on Chat-Completions-normalized paths
}

// `usage()` is a base dimension of ResponseMeta (every response can report it).
// `Supported<T>` is reserved for dimensions a provider genuinely CAN'T produce
// (logprobs on Anthropic, etc.); usage is always present, so it returns Usage
// directly. The per-provider key divergence is normalized LAZILY inside each
// concrete response type's `implements ResponseMeta` (see usage.baml).
interface ResponseMeta {
  function usage(self) -> Usage
}

// The tool-loop analog of `meta_of(body)`. `Tools.step` returns `T | ToolCalls`
// (a sentinel SUM, not a `call_with` product), so per-turn usage cannot ride
// `call_with` inside the loop — there is no single `T` to pair a turn with. We
// add `meta(t) -> ResponseMeta` on the SAME ResponseMeta grain instead, exposing
// the most recent turn's normalized response. Like `Traceable requires Provider`
// (we cannot edit the canonical `Tools` interface), this is a sibling extension
// `MeteredTools requires Tools` — net-new, flagged in evaluation.md — so a value
// typed as `MeteredTools` can actually call `.meta()` (a base-`Tools` value
// cannot). The combinator `match`es for it and degrades when a provider's tool
// loop can't report per-turn usage.
interface MeteredTools requires Tools {
  function meta(self, t: Transcript) -> ResponseMeta
      throws baml.ExtendUnknownError<baml.errors.ToolError>
}

// Cost attribution: usage * a per-model price table -> dollars. Pure BAML.
class Price { input_per_mtok: float, output_per_mtok: float, cache_read_per_mtok: float }

class Cost {
  function of(usage: Usage, price: Price) -> float {       // static, no self
    let m = 1000000.0;
    (usage.input_tokens / m) * price.input_per_mtok
      + (usage.output_tokens / m) * price.output_per_mtok
      + (usage.cache_read_tokens / m) * price.cache_read_per_mtok
  }
  function attrs(usage: Usage, dollars: float) -> map<string, string> {  // gen_ai.* conventions
    {
      "gen_ai.usage.input_tokens": usage.input_tokens.to_string(),
      "gen_ai.usage.output_tokens": usage.output_tokens.to_string(),
      "gen_ai.usage.cache_read_tokens": usage.cache_read_tokens.to_string(),
      "gen_ai.usage.reasoning_tokens": usage.reasoning_tokens.to_string(),
      "gen_ai.usage.cost_usd": dollars.to_string(),
    }
  }
}

// -----------------------------------------------------------------------------
// Wire-message vs UI-message.
//
// The model only ever sees the WIRE shape (lean, normalized). Observability
// PERSISTS the UI shape (rich: ids, timestamps, tool-call states, reasoning).
// These are different types on purpose; conversion is one-way per direction.
// -----------------------------------------------------------------------------

class WireMessage {                 // == what build_request serializes; lean
  role: string                      // "system" | "user" | "assistant" | "tool"
  content: string
}

class UiPart {                      // a renderable fragment of an assistant turn
  kind: string                      // "text" | "tool_call" | "tool_result" | "reasoning"
  text: string
  tool_id: string?                  // present for tool_call / tool_result
}

class UiMessage {                   // == what we persist + render; rich
  id: string
  role: string
  created_at: Instant
  parts: UiPart[]
  redacted: bool                    // whether `parts` have already been scrubbed
}

class MessageView {
  // wire -> ui is NOT free: the rich shape carries data the wire never had
  // (ids, timestamps, split parts). We can only reconstruct a coarse UiMessage
  // from the wire, which is exactly why the SDKs keep them separate.
  function ui_from_wire(w: WireMessage, id: string) -> UiMessage {
    UiMessage {
      id: id,
      role: w.role,
      created_at: obs_now(),
      parts: [ UiPart { kind: "text", text: w.content, tool_id: null } ],
      redacted: false,
    }
  }
  // ui -> wire IS lossy-by-design: drop ids/timestamps/reasoning, flatten parts.
  // This is the `convertToModelMessages` direction.
  function wire_from_ui(u: UiMessage) -> WireMessage {
    let mut buf = "";
    for (let p in u.parts) {
      if (p.kind == "reasoning") { continue; }       // model never re-sees its own scratchpad
      buf = buf + p.text;
    }
    WireMessage { role: u.role, content: buf }
  }
  // Redaction happens on the UI shape, BEFORE export. Returns a copy flagged.
  function redacted_copy(u: UiMessage) -> UiMessage {
    let mut parts: UiPart[] = [];
    for (let p in u.parts) {
      parts = parts + [ UiPart { kind: p.kind, text: redact(p.text), tool_id: p.tool_id } ];
    }
    UiMessage { id: u.id, role: u.role, created_at: u.created_at, parts: parts, redacted: true }
  }
}

// -----------------------------------------------------------------------------
// The Traced combinator.
//
// Wraps ANY Provider, opens a span per call, forwards every capability. Like
// Retry/Cache, it is a plain non-generic class living at the combinator layer,
// and it claims the capabilities it forwards (degrading/erroring per member).
//
// `export_messages` decides whether to persist the (redacted) UI message — the
// sampling/PII knob the background calls out.
// -----------------------------------------------------------------------------

class Traced {
  inner: Provider
  name: string                      // span name, e.g. "llm.call" or the function id
  parent: SpanId?                   // nest under an agent-loop parent when present
  price: Price?                     // for cost attribution; null => skip cost
  export_messages: bool             // sampling/PII gate: persist the UI message?

  // Turn a projected Usage into the gen_ai.* span attributes (cost folded in).
  function usage_attrs(self, usage: Usage) -> map<string, string> {
    match (self.price) {
      let p: Price => Cost.attrs(usage, Cost.of(usage, p)),
      _            => Cost.attrs(usage, 0.0),
    }
  }

  // `call` + `call_with` now belong to HttpProvider (provider-as-marker): the
  // base `Provider` is a bare marker, so request/response lives here. This block
  // FORWARDS both to the inner provider's call_with (the P1 fix: usage/sidecar is
  // not dropped — it rides the winner's codec pass), so we KEEP it as a combinator
  // delegation rather than fabricating build_request/send/parse/meta_of.
  implements HttpProvider {
    type Body = unknown
    // `call` stays the zero-tax path; usage is read by `call_with` below.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (value, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      value
    }

    // The metadata path. We project the usage sidecar OUT of the same codec pass
    // and attach it to the span — no stateful `last_usage()` stash, no race under
    // Fallback (the inner combinator forwards `call_with`, so usage is the
    // WINNER's view, not a separately-fetched body). The projection the caller
    // asked for (`project`) is honored independently of the span attribution.
    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // `self.inner` is the bare `Provider` marker (no `call_with`), so route the
      // projection to the member that owns request/response — `HttpProvider` —
      // and FORWARD it (so usage/sidecar is the WINNER's view, not dropped). A
      // pure-realtime/harness inner has no call to wrap: that errors Unsupported.
      match (self.inner) {
        let h: HttpProvider => {
          let span = obs_start(self.name, self.parent, { "gen_ai.operation.name": "chat" });
          // one pass: project BOTH the caller's U and the usage we need for the span.
          let (value, both): (T, (U, Usage)) =
            h.call_with<T, (U, Usage)>(prompt, m => (project(m), m.usage())) catch (e) {
              _ => {
                // partial failure must still CLOSE the span, with an error status,
                // then re-raise the original error unchanged (Refused, rate-limit,
                // foreign Io) on the call channel.
                obs_finish(span, {}, "call failed");
                throw baml.UnknownError.with_message<baml.errors.CallError>(e, "traced call failed");
              }
            };
          let (u, usage): (U, Usage) = both;
          obs_finish(span, self.usage_attrs(usage), null);
          return (value, u);
        },
        _ => throw baml.errors.Unsupported { message: "inner provider cannot serve a request/response call" },
      }
    }
  }

  // ---- Streaming span: open on first byte, close on last, usage at the end. --
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      match (self.inner) {
        let s: Streaming => {
          let span = obs_start(self.name, self.parent, { "gen_ai.operation.name": "chat.stream" });
          let raw = s.stream<TStream, TFinal>(prompt);
          // Wrap the stream so we close the span on the LAST item — or on a
          // mid-stream error, so no span dangles. TokensKnownAtEnd: usage_attrs
          // is read at on_done, when the final chunk has arrived.
          TracedStream<TStream, TFinal> {
            inner: raw, span: span, opened: false, owner: self,
          }
        },
        // degrade: no native streaming -> buffer the call, still one span.
        // `self.call` throws on the CallError channel; this method declares the
        // StreamError channel, so a foreign CallError (a refusal, a rate-limit)
        // is re-channeled through baml.UnknownError to stay legal here.
        _ => {
          let final: TFinal = self.call<TFinal>(prompt) catch (e) {
            _ => throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "traced stream degrade-to-call failed");
          };
          baml.llm.buffer_as_stream<TStream, TFinal>(final)
        },
      }
    }
  }

  // ---- Tools: nest a child span per model turn + per dispatch under a parent. -
  implements Tools {
    type Transcript = TracedTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> TracedTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      match (self.inner) {
        // require MeteredTools (not just Tools): we need `meta(t)` to project
        // per-turn usage onto the span. A provider whose tool loop can't report
        // usage is rejected here, same as the missing-Tools case.
        let t: MeteredTools => {
          let loop_span = obs_start(self.name, self.parent, { "gen_ai.operation.name": "agent.loop" });
          TracedTranscript {
            inner: t.begin<T>(prompt, tools),
            owner_tools: t,
            loop_span: loop_span,
            owner: self,
            turn: 0,
          }
        },
        _ => throw baml.errors.Unsupported { message: "inner provider has no metered Tools capability" },
      }
    }

    function step<T>(self, tr: TracedTranscript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // one child span per MODEL turn, nested under the loop span.
      let turn_span = obs_start("llm.turn", tr.loop_span, { "turn": tr.turn.to_string() });
      let out: T | ToolCalls = tr.owner_tools.step<T>(tr.inner) catch (e) {
        _ => {
          obs_finish(turn_span, {}, "turn failed");
          throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "turn failed");
        }
      };
      // Per-turn usage rides the transcript's ResponseMeta (the Tools capability
      // exposes `meta(t)` for the last turn) — same projection grain as call_with,
      // not a stateful `last_usage()` stash on the provider.
      obs_finish(turn_span, self.usage_attrs(tr.owner_tools.meta(tr.inner).usage()), null);
      out
    }

    function submit(self, tr: TracedTranscript, results: ToolResult[]) -> TracedTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // the dispatch spans were already emitted by ExecutionContext.emit during
      // ctx.dispatch (see TracingContext below); here we only thread the loop.
      TracedTranscript {
        inner: tr.owner_tools.submit(tr.inner, results),
        owner_tools: tr.owner_tools,
        loop_span: tr.loop_span,
        owner: self,
        turn: tr.turn + 1,
      }
    }

    // We DON'T override run_tools — the default loop in `Tools` drives
    // begin/step/submit, so our spans nest automatically. But we DO need the
    // loop span to close when the loop returns; the default run_tools returns T
    // directly, so we close in `step` on the value arm via a wrapper:
    function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let tr = self.begin<T>(prompt, tools);
      let mut cur = tr;
      while (true) {
        match (self.step<T>(cur)) {
          let calls: ToolCalls => { cur = self.submit(cur, ctx.dispatch(calls.calls)); },
          let value: T => {
            // close the parent span with the final turn's usage, projected from
            // the transcript's ResponseMeta (the loop-aggregate caveat: this is
            // the last turn's view, see evaluation.md).
            obs_finish(cur.loop_span, self.usage_attrs(cur.owner_tools.meta(cur.inner).usage()), null);
            return value;
          },
        }
      }
      baml.sys.panic("unreachable")
    }
  }
}

// Provider.with_traced — a fluent default method so apps write `p.with_traced(...)`.
// (We can't EDIT the canonical Provider interface here, so this is an extension
// the std lib adds alongside with_retry / cached.)
interface Traceable requires Provider {
  function with_traced(self, name: string, price: Price?) -> Traced {
    Traced { inner: self, name: name, parent: null, price: price, export_messages: false }
  }
}

// -----------------------------------------------------------------------------
// TracedStream — closes the span on the last item or on mid-stream error.
// -----------------------------------------------------------------------------
class TracedStream<TStream, TFinal> {
  inner: baml.llm.Stream<TStream, TFinal>
  span: SpanId
  opened: bool
  owner: Traced

  implements baml.llm.StreamLike {
    type Item = TStream
    type Final = TFinal

    function next(self) -> TStream | baml.llm.StreamDone
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      // mark first-byte the first time through.
      if (!self.opened) { obs_event(self.span, "first_token", {}); }
      let item: TStream | baml.llm.StreamDone = self.inner.next() catch (e) {
        _ => {
          obs_finish(self.span, {}, "stream broke mid-flight");
          throw baml.UnknownError.with_message<baml.errors.StreamError>(e, "stream error");
        }
      };
      match (item) {
        let done: baml.llm.StreamDone => {
          // last byte: NOW usage is known. The terminal StreamDone carries the
          // normalized ResponseMeta (the streaming analog of `meta_of(body)`),
          // so usage is projected here — same grain as call_with, not a stash.
          obs_finish(self.span, self.owner.usage_attrs(done.meta.usage()), null);
          done
        },
        let chunk: TStream => chunk,
      }
    }
  }
}

class TracedTranscript {
  inner: unknown          // the wrapped provider's own opaque transcript
  owner_tools: MeteredTools   // MeteredTools so `.meta(t)` is reachable through this field
  loop_span: SpanId
  owner: Traced
  turn: int
}

// -----------------------------------------------------------------------------
// TracingContext — an ExecutionContext whose `dispatch` emits a function_span
// per tool call, nested under the agent-loop span. This is the
// "ExecutionContext gains an event-emit surface" flag from §7, made concrete.
// -----------------------------------------------------------------------------
class TracingContext {
  parent_span: SpanId        // the agent-loop span to nest tool spans under
  inner_dispatch: (ToolCall[]) -> ToolResult[]    // the app's real tool runner

  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[]
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let mut results: ToolResult[] = [];
      for (let c in calls) {
        // one function_span per tool call; id-correlated to the result.
        let tspan = obs_start("tool." + c.name, self.parent_span, { "tool.call_id": c.id });
        let one: ToolResult[] = self.inner_dispatch([c]) catch (e) {
          _ => {
            obs_finish(tspan, {}, "tool threw");
            throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "tool dispatch failed");
          }
        };
        obs_finish(tspan, { "tool.result_id": c.id }, null);
        results = results + one;
      }
      results
    }

    // The emit surface: the loop / provider can drop point events onto the
    // current loop span (e.g. "model_requested_3_tools").
    function emit(self, name: string, attrs: map<string, string>) -> null {
      obs_event(self.parent_span, name, attrs)
    }
  }
}
