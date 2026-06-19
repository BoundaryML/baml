// ===========================================================================
// 16 — The agent security threat model (lethal trifecta)
// implement.baml  —  STD-LIB / library-author code
//
// Builds on the canonical primitives (Provider, Tools, ExecutionContext,
// ToolCall/ToolResult). Adds the *only* thing the providers refuse to give us:
// a content trust boundary. No major API marks a tool result `trusted: false`,
// so taint is entirely the caller's to build. We model it as:
//
//   1. A `Tainted<T>` provenance wrapper around a value (host-owned set of
//      taint labels that survives transformation).
//   2. A `Provenance` capability on `Tools`, so a provider's loop can ATTACH
//      taint to the tool results it threads back into context.
//   3. Combinators that BREAK the trifecta: `Quarantined` (strips Tools => a
//      no-tools provider), and a `GatedContext` ExecutionContext that does
//      provenance-aware approval + output-URL allowlisting at dispatch time.
//
// The teaching point: the provider layer cannot give a compile-time guarantee
// that untrusted bytes never reach an exfil tool. The best the model can do is
// (a) make "no tools" a *type-level* fact (capability subtraction) and (b) make
// the taint check a *runtime* gate the dispatcher cannot be talked out of.
// ===========================================================================

// ---------------------------------------------------------------------------
// Net-new host primitive: taint labels + a value carrying them.
//
// A taint label is just a provenance string ("web:evil.example", "mcp:notion",
// "email:inbox"). `Tainted<T>` carries the labels alongside the value; the set
// is host-owned ($rust_type) so it survives being passed through `unknown`
// boundaries in ToolResult.output without BAML having to thread it structurally.
// ---------------------------------------------------------------------------
class TaintSet { _labels: $rust_type }              // opaque set<string>, host-owned

class Tainted<T> {
  value: T
  taint: TaintSet

  // pure constructors
  function clean(v: T) -> Tainted<T> { Tainted<T> { value: v, taint: TaintSet.empty() } }
  function from(v: T, label: string) -> Tainted<T> {
    Tainted<T> { value: v, taint: TaintSet.of(label) }
  }
  function is_tainted(self) -> bool { !self.taint.is_empty() }

  // The viral part: any transform that consumes a Tainted value re-emits the
  // union of incoming taint. This is the only honest way to keep taint attached
  // across summarization/embedding (see evaluation "taint is easy to launder").
  function map<U>(self, f: (T) -> U) -> Tainted<U> {
    Tainted<U> { value: f(self.value), taint: self.taint }
  }
}

class TaintSet {
  function empty() -> TaintSet { $rust_io_function }
  function of(label: string) -> TaintSet { $rust_io_function }
  function is_empty(self) -> bool { $rust_io_function }
  function union(self, other: TaintSet) -> TaintSet { $rust_io_function }
  function labels(self) -> string[] { $rust_io_function }
}

// ---------------------------------------------------------------------------
// Net-new capability: Provenance.
//
// `requires Tools` (and therefore Provider). A Tools provider that ALSO knows
// how a given tool result was produced can label it. The default impl labels
// nothing (matches reality: most providers can't). A provider that wraps web
// fetch / MCP overrides `label_for` to tag results by source.
//
// Crucially this lives BELOW the ToolCall/ToolResult seam: the loop in `Tools`
// never sees taint; the dispatcher (ExecutionContext) does. So a provider opts
// into provenance without changing the wire-threading code in begin/step/submit.
// ---------------------------------------------------------------------------
interface Provenance requires Tools {
  // Given a finished tool call, what taint do its results carry? Default: none.
  function label_for(self, call: ToolCall) -> TaintSet { TaintSet.empty() }
}

// ---------------------------------------------------------------------------
// A real provider with divergence #1: an HTTP chat model that ALSO discovers
// tools from MCP servers, and therefore can attach provenance. This is the
// "privileged" model — it has tools, so it must be the one we lock down.
//
// Divergence shown inside the body: OpenAI-style id-correlation in the Tools
// loop (every ToolCall has a real `id`), and source labelling that distinguishes
// MCP tool output from web-fetch output.
// ---------------------------------------------------------------------------
class OpenAITools {
  model: string
  api_key: string
  mcp_servers: string[]                              // remote MCP endpoints discovered at connect time

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

  implements Tools {
    // Opaque, provider-owned. Holds the assistant-turn history + OpenAI call ids.
    type Transcript = $rust_type

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript throws baml.errors.LlmClient {
      $rust_io_function                              // threads system+user turn, registers MCP tools by id
    }
    function step<T>(self, t: Transcript) -> T | ToolCalls throws baml.errors.LlmClient {
      $rust_io_function                              // each ToolCall carries the OpenAI tool_call_id verbatim
    }
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript throws baml.errors.LlmClient {
      $rust_io_function                              // correlates ToolResult.id -> tool_call_id (id-based)
    }
    // run_tools inherited from Tools default — dispatch policy (taint/gate) lives in ctx.
  }

  implements Provenance {
    // The provider knows which of its tools are MCP-sourced vs web-fetch vs local,
    // so it can label results by provenance. THIS is the seam the APIs don't give us.
    function label_for(self, call: ToolCall) -> TaintSet {
      // web fetch + MCP output is untrusted-by-construction; local compute is clean.
      if (call.name == "web_fetch" || call.name == "web_search") {
        TaintSet.of("web:" + host_of(call))
      } else if (is_mcp_tool(self, call.name)) {
        TaintSet.of("mcp:" + call.name)
      } else {
        TaintSet.empty()
      }
    }
  }

  function from_model(model: string, mcp: string[]) -> OpenAITools {
    OpenAITools { model: model, api_key: env.OPENAI_API_KEY, mcp_servers: mcp }
  }
}

// host helpers used by label_for
function host_of(call: ToolCall) -> string { $rust_io_function }     // pulls url host from call.args
function is_mcp_tool(p: OpenAITools, name: string) -> bool { $rust_io_function }

// ---------------------------------------------------------------------------
// A real provider with divergence #2: Gemini. Same provenance story, but the
// Tools loop correlates results by NAME, not id (Gemini ships no call ids). We
// show that the taint machinery is indifferent to this divergence: provenance
// keys off call.name (always present) so it works for both id- and name-based
// providers unchanged.
// ---------------------------------------------------------------------------
class GeminiTools {
  model: string
  api_key: string

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                              // tools -> OpenAPI-3 subset; schema -> responseSchema
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Tools {
    type Transcript = $rust_type
    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript throws baml.errors.LlmClient {
      $rust_io_function
    }
    function step<T>(self, t: Transcript) -> T | ToolCalls throws baml.errors.LlmClient {
      // Gemini gives no id; the provider SYNTHESIZES a stable id from name+index so
      // the opaque-id contract holds, but correlation on submit is name-based.
      $rust_io_function
    }
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript throws baml.errors.LlmClient {
      $rust_io_function                              // functionResponse parts, correlated by tool NAME
    }
  }

  implements Provenance {
    function label_for(self, call: ToolCall) -> TaintSet {
      if (call.name == "web_fetch" || call.name == "web_search") { TaintSet.of("web:" + host_of(call)) }
      else { TaintSet.empty() }
    }
  }
}

// ---------------------------------------------------------------------------
// Net-new combinator: Quarantined.  Breaks the trifecta by CAPABILITY SUBTRACTION.
//
// It wraps a Provider and forwards `call` only. It deliberately does NOT
// implement Tools. So `Quarantined(p)` is, at the type level, a no-tools
// provider: a `.run_tools` companion match against `Tools` will miss it and the
// `_` arm fires. This is the dual-LLM/quarantine pattern expressed as a type:
// the quarantined model reads untrusted bytes but structurally cannot act.
//
// Note what this costs (the §7 "breaking the trifecta costs capability"): you
// lose Streaming, Realtime, Tools — everything except the irreducible `call`.
// That loss is the point; it is enforced, not advisory.
// ---------------------------------------------------------------------------
class Quarantined {
  inner: Provider

  implements Provider {
    // Only `call`. Whatever the quarantined model returns is born TAINTED, because
    // it was produced from untrusted content. We can't change `call`'s return type
    // generically, so the obligation is on the CALLER to wrap (see usage.baml's
    // `summarize_untrusted`). The combinator's job is purely to deny tools.
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
    }
  }
  // intentionally NO `implements Tools`, NO `implements Streaming`, NO Realtime.
}

interface Provider {
  // fluent sugar so `p.quarantined()` reads like the other combinators.
  function quarantined(self) -> Quarantined { Quarantined { inner: self } }
}

// ---------------------------------------------------------------------------
// Net-new ExecutionContext: GatedContext.
//
// The canonical `Tools.run_tools` calls `ctx.dispatch(calls)`. The base
// ExecutionContext just runs tools. GatedContext is the provenance-aware,
// allowlist-enforcing dispatcher that attacks leg (3) of the trifecta:
//
//   * taint tracking      — results inherit Provenance.label_for of their call;
//                           taint is propagated into any downstream call whose
//                           args reference a tainted prior result.
//   * URL allowlisting     — a call carrying an outbound URL whose host isn't on
//                           the allowlist is REFUSED (covers POST-to-attacker AND
//                           markdown-image smuggling once the renderer is fed
//                           through `scrub_output`).
//   * provenance-aware gate — a side-effecting / exfil-capable call is only sent
//                           to the human approver when its args are TAINTED.
//                           "tainted input + dangerous tool", not tool name alone.
//
// It needs the originating Provider to ask `label_for`, so it holds a Provenance.
// ---------------------------------------------------------------------------
class GatedContext {
  source: Provenance                  // the privileged provider, for label_for
  allow_hosts: string[]               // output-URL allowlist (leg 3)
  approver: Approver                  // human-in-the-loop, fires only on taint
  _taint_ledger: $rust_type           // host-owned map<result-id -> TaintSet>, the running taint state

  implements ExecutionContext {
    function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient {
      let out: ToolResult[] = [];
      for (let call in calls) {
        // (1) compute incoming taint: provenance of THIS source + any tainted args
        //     referenced from prior results (viral propagation through the ledger).
        let arg_taint: TaintSet = taint_of_args(self._taint_ledger, call);

        // (2) output-URL allowlist — refuse exfil hosts outright, before dispatch.
        for (let url in outbound_urls(call)) {
          if (!host_allowed(self.allow_hosts, url)) {
            throw baml.errors.LlmClient {
              message: "blocked outbound URL to non-allowlisted host: " + url
            };
          }
        }

        // (3) provenance-aware approval gate: only interrupt the human when the
        //     trifecta is actually assembled here — tainted input + dangerous tool.
        if (is_side_effecting(call) && !arg_taint.is_empty()) {
          let decision = self.approver.review(call, arg_taint.labels());
          if (!decision.approved) {
            throw baml.errors.LlmClient {
              message: "human denied tainted call to " + call.name
            };
          }
        }

        // (4) run the tool, then LABEL its output with this source's provenance
        //     UNIONED with the args' taint (taint is viral: a clean tool fed
        //     tainted args still launders nothing — output stays tainted).
        let raw: unknown = run_tool(call);
        let result_taint: TaintSet = self.source.label_for(call).union(arg_taint);
        record_taint(self._taint_ledger, call.id, result_taint);

        out = push(out, ToolResult { id: call.id, output: raw });
      }
      out
    }
  }
}

interface Approver {
  function review(self, call: ToolCall, taint: string[]) -> ApprovalDecision throws baml.errors.LlmClient
}
class ApprovalDecision { approved: bool, reason: string? }

// host helpers for the gate
function taint_of_args(ledger: $rust_type, call: ToolCall) -> TaintSet { $rust_io_function }
function outbound_urls(call: ToolCall) -> string[] { $rust_io_function }   // URLs in tool args
function host_allowed(allow: string[], url: string) -> bool { $rust_io_function }
function is_side_effecting(call: ToolCall) -> bool { $rust_io_function }   // best-effort: name + arg shape
function run_tool(call: ToolCall) -> unknown { $rust_io_function }
function record_taint(ledger: $rust_type, id: string, t: TaintSet) -> null { $rust_io_function }
function push(xs: ToolResult[], x: ToolResult) -> ToolResult[] { $rust_io_function }

// ---------------------------------------------------------------------------
// Net-new: output scrubbing for the markdown-image / autolink channel.
//
// Markdown rendering is client-dependent, so the model API never sees the
// exfil. This is a pure post-processing pass the APP runs on a model's final
// answer before handing it to a renderer: strip any URL whose host isn't on the
// allowlist. Same allowlist as the dispatcher — leg (3), the rendering half.
// ---------------------------------------------------------------------------
function scrub_output(answer: string, allow_hosts: string[]) -> string {
  $rust_io_function                                  // rewrites/strips ![](non-allowlisted) and autolinks
}
