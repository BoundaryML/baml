// =============================================================================
// Observability — APPLICATION author code.
//
// Shows how little the app writes to get spans + cost + redacted message
// capture, across two providers that DIVERGE on usage reporting and streaming:
//   - OpenAI  (usage.prompt_tokens / completion_tokens; native SSE)
//   - Anthropic (usage.input_tokens / output_tokens + cache_*; native SSE)
//
// The point: the app wraps with `.with_traced(...)` and never touches the
// per-provider key differences — UsageReporting normalizes them below the seam.
// =============================================================================

// ---- Two divergent providers, each teaching their usage encoding. -----------

class OpenAI {
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

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient { $rust_io_function }
  }

  // DIVERGENCE: OpenAI reports prompt_tokens / completion_tokens, no cache split
  // on the Chat-Completions path, no reasoning tokens.
  implements UsageReporting {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      let raw = self.last_response_body() catch (e) { _ => { throw baml.errors.LlmClient { message: "no usage" }; } };
      Usage {
        input_tokens: baml.json.int(raw, "usage.prompt_tokens"),
        output_tokens: baml.json.int(raw, "usage.completion_tokens"),
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        reasoning_tokens: 0,
      }
    }
  }
  function last_response_body(self) -> string throws baml.errors.LlmClient { $rust_io_function }
}

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

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

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient { $rust_io_function }
  }

  implements Tools {
    type Transcript = $rust_type        // provider-owned, opaque (Anthropic positional tool_result order)
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> $rust_type throws baml.errors.LlmClient { $rust_io_function }
    function step<T>(self, t: $rust_type) -> T | ToolCalls throws baml.errors.LlmClient { $rust_io_function }
    function submit(self, t: $rust_type, results: ToolResult[]) -> $rust_type throws baml.errors.LlmClient { $rust_io_function }
  }

  // DIVERGENCE: Anthropic reports input_tokens / output_tokens AND
  // cache_creation_input_tokens / cache_read_input_tokens. Normalized to ONE Usage.
  implements UsageReporting {
    function last_usage(self) -> Usage throws baml.errors.LlmClient {
      let raw = self.last_response_body() catch (e) { _ => { throw baml.errors.LlmClient { message: "no usage" }; } };
      Usage {
        input_tokens: baml.json.int(raw, "usage.input_tokens"),
        output_tokens: baml.json.int(raw, "usage.output_tokens"),
        cache_read_tokens: baml.json.int(raw, "usage.cache_read_input_tokens"),
        cache_write_tokens: baml.json.int(raw, "usage.cache_creation_input_tokens"),
        reasoning_tokens: 0,
      }
    }
  }
  function last_response_body(self) -> string throws baml.errors.LlmClient { $rust_io_function }
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
// the span because Anthropic's UsageReporting populates them.
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
function run_research(question: string, tools: Tool[], real_dispatch: (ToolCall[]) -> ToolResult[]) -> Summary {
  let p = ResearchAgent_provider();        // the client body, as a provider value
  match (p) {
    let t: Tools => {
      // open the parent loop span ourselves so the ctx can nest under it.
      let loop_span = obs_start("research.run", null, { "question": redact(question) });
      let ctx = TracingContext { parent_span: loop_span, inner_dispatch: real_dispatch };
      let answer: Summary = t.run_tools<Summary>(render_prompt(question), tools, ctx) catch (e) {
        _ => { obs_finish(loop_span, {}, "agent failed"); throw baml.errors.LlmClient { message: "agent failed" }; }
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
