// =============================================================================
// Observability — APPLICATION author code.
//
// Shows how little the app writes to get spans + cost + redacted message
// capture, across two providers that DIVERGE on usage reporting and streaming:
//   - OpenAI  (usage.prompt_tokens / completion_tokens; native SSE)
//   - Anthropic (usage.input_tokens / output_tokens + cache_*; native SSE)
//
// The point: the app wraps with `.with_traced(...)` and never touches the
// per-provider key differences — each provider's ResponseMeta impl normalizes
// them lazily below the seam, projected out of `call_with`.
// =============================================================================

// ---- Concrete response types normalize usage LAZILY via ResponseMeta. --------
// Usage is a PRODUCT sidecar: it rides `call_with` out of the same codec pass as
// the value (via `meta_of(body)`), NOT a stateful `last_usage()` companion. So
// there is no "made a call but stashed no body" failure mode to classify — the
// body that holds the tokens is the very body `parse<T>` consumed, projected in
// the same pass. The bespoke `baml.errors.UsageError` channel and the
// `*UsageMissingError` classes are therefore deleted: the only way usage is
// "missing" now is a malformed body, which surfaces on the CallError channel
// like any other parse failure.

// ---- Two divergent providers, each teaching their usage encoding. -----------

class OpenAI {
  model: string
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai http send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai parse failed");
    }
    // The sidecar seam: wrap the same Body the call just parsed in a lazy
    // ResponseMeta. `call_with` (on HttpProvider) projects over this; nothing is
    // stashed on `self`, so a Fallback forwards it instead of racing a stash.
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      OpenAIResponse { raw: from }
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> { $rust_io_function }
  }
}

// DIVERGENCE: OpenAI reports prompt_tokens / completion_tokens, no cache split
// on the Chat-Completions path, no reasoning tokens. Normalized LAZILY: `usage()`
// is only computed when a projection asks for it.
//
// READING the wire body: `baml.http.Response` has no `.json_path_int` (that was
// invented). We decode the body into a typed shape with `baml.json.from_json<T>`
// (preferred over hand-navigating the `json` union) and read its fields. The
// provider-specific key names live on the shape class, so the divergence is
// captured by the type, not a path string.
class OpenAiUsageWire {                 // mirrors OpenAI's `usage` object
  prompt_tokens: int
  completion_tokens: int
}
class OpenAiBodyWire {                  // the slice of the response body we read
  usage: OpenAiUsageWire
}

class OpenAIResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let body: OpenAiBodyWire = baml.json.from_json<OpenAiBodyWire>(
        baml.json.parse(self.raw.text())
      ) catch (e) {
        _ => { return Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0, reasoning_tokens: 0 }; }
      };
      Usage {
        input_tokens: body.usage.prompt_tokens,
        output_tokens: body.usage.completion_tokens,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
      }
    }
  }
}

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.http.send(request) catch (e) {
        _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic http send failed");
      }
    }
    function parse<T>(self, from: baml.http.Response) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      baml.sap.parse<T>(from.text())
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "anthropic parse failed");
    }
    function meta_of(self, from: baml.http.Response) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      AnthropicResponse { raw: from }
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> { $rust_io_function }
  }

  implements Tools {
    type Transcript = $rust_type        // provider-owned, opaque (Anthropic positional tool_result order)
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> $rust_type
        throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
    function step<T>(self, t: $rust_type) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
    function submit(self, t: $rust_type, results: ToolResult[]) -> $rust_type
        throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
  }

  // The tool-loop analog of `meta_of(body)`: project the last turn's usage off
  // the transcript. `step` returns `T | ToolCalls` (a sentinel SUM, not a
  // call_with product), so per-turn usage cannot ride `call_with` here — the
  // transcript exposes a ResponseMeta for the most recent turn instead. This is
  // the `MeteredTools requires Tools` sibling extension (net-new, flagged in
  // evaluation.md), declared in its OWN `implements` block so it stays off the
  // canonical `Tools` interface; it reuses the SAME ResponseMeta grain.
  implements MeteredTools {
    function meta(self, t: $rust_type) -> ResponseMeta
        throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
  }

}

// DIVERGENCE: Anthropic reports input_tokens / output_tokens AND
// cache_creation_input_tokens / cache_read_input_tokens. Normalized to ONE Usage,
// LAZILY, from the same body the call parsed. Same `from_json<T>` decode as
// OpenAI, onto a shape whose field names are Anthropic's.
class AnthropicUsageWire {              // mirrors Anthropic's `usage` object
  input_tokens: int
  output_tokens: int
  cache_read_input_tokens: int
  cache_creation_input_tokens: int
}
class AnthropicBodyWire {
  usage: AnthropicUsageWire
}

class AnthropicResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      let body: AnthropicBodyWire = baml.json.from_json<AnthropicBodyWire>(
        baml.json.parse(self.raw.text())
      ) catch (e) {
        _ => { return Usage { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0, reasoning_tokens: 0 }; }
      };
      Usage {
        input_tokens: body.usage.input_tokens,
        output_tokens: body.usage.output_tokens,
        cache_read_tokens: body.usage.cache_read_input_tokens,
        cache_write_tokens: body.usage.cache_creation_input_tokens,
        reasoning_tokens: 0,
      }
    }
  }
}

// ---- Price tables (pure data, app-owned). -----------------------------------

function gpt4o_price() -> Price { Price { input_per_mtok: 2.5, output_per_mtok: 10.0, cache_read_per_mtok: 1.25 } }
function sonnet_price() -> Price { Price { input_per_mtok: 3.0, output_per_mtok: 15.0, cache_read_per_mtok: 0.30 } }

// ---- Clients: one line to wrap any provider with tracing. -------------------
// `.with_traced` is the Traceable default method; the price enables cost attribution.

client TracedGpt() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
    .with_traced("q3-summary", gpt4o_price())
}

client TracedClaude() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
    .with_traced("q3-summary", sonnet_price())
}

// Combinator chain reads outside-in: trace wraps fallback wraps retry.
// The SPAN sees the whole orchestration as one logical call; the retries/
// fallbacks happen inside `inner.call`.
client TracedResilient() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
    .with_retry(2)
    .fallback_to(Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 })
    .with_traced("q3-summary", gpt4o_price())
}

// ---- App functions. The app author writes a normal function; the span is free.

class Summary { headline: string, bullets: string[] }

// Plain call: one generation_span with usage + cost, regardless of provider.
function SummarizeRevenue(quarter: string) -> Summary {
  client: TracedGpt()
  prompt #"Summarize {{ quarter }} revenue."#
}

// Swap to Claude with ZERO code change downstream — cache tokens now appear on
// the span because Anthropic's `AnthropicResponse` ResponseMeta populates them.
function SummarizeRevenueClaude(quarter: string) -> Summary {
  client: TracedClaude()
  prompt #"Summarize {{ quarter }} revenue."#
}

// Streaming companion: the span opens on first token, closes on last, with
// usage attached at the end — the app just calls .stream.
//   SummarizeRevenue.stream("Q3")
//
// Tool loop companion: the loop span nests a turn span per model call and a
// function_span per tool dispatch, because the app hands in a TracingContext.
function ResearchAgent(question: string, tools: Tool[]) -> Summary {
  client: Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 4096 }
            .with_traced("research.loop", sonnet_price())
  prompt #"Answer with tools: {{ question }}"#
}

// How the app drives the traced tool loop with nested spans + redacted export.
// The TracingContext's parent_span is the loop span the Traced.Tools impl opened.
function run_research(question: string, tools: Tool[], real_dispatch: (ToolCall[]) -> ToolResult[]) -> Summary
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let p = ResearchAgent_provider();        // the client body, as a provider value
  match (p) {
    let t: Tools => {
      // open the parent loop span ourselves so the ctx can nest under it.
      let loop_span = obs_start("research.run", null, { "question": redact(question) });
      let ctx = TracingContext { parent_span: loop_span, inner_dispatch: real_dispatch };
      let answer: Summary = t.run_tools<Summary>(render_prompt(question), tools, ctx) catch (e) {
        _ => {
          obs_finish(loop_span, {}, "agent failed");
          throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "agent failed");
        }
      };
      obs_finish(loop_span, {}, null);
      answer
    },
    _ => throw baml.errors.Unsupported { message: "client is not a tool-runner" },
  }
}

// ---- Wire-vs-UI: persisting a turn for the UI without leaking it to the model.
// The app builds a rich UiMessage to persist/render, then DERIVES the lean wire
// message to feed back to the model — reasoning parts dropped, PII redacted
// before export.
function persist_and_continue(assistant: UiMessage) -> WireMessage {
  // 1. export the RICH message (redacted) to the observability backend.
  let for_export = MessageView.redacted_copy(assistant);
  obs_event(obs_start("persist", null, {}), "ui_message", { "id": for_export.id });
  // 2. feed the LEAN message to the model — never the reasoning, never the ids.
  MessageView.wire_from_ui(assistant)
}
