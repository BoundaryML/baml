// ============================================================================
// 13 — Scaling the catalog: deferred & searchable tools  ::  implement.baml
//      (LIBRARY AUTHOR code)
// ============================================================================
//
// Builds on the canonical primitives from _conventions.md VERBATIM:
//   interface Provider          — call<T>
//   interface HttpProvider      — build_request / send / parse / default call
//   interface Tools             — begin / step / submit / default run_tools
//   class ToolCall   { id, name, args }                 // id OPAQUE to the loop
//   class ToolResult { id, output }
//   class ToolCalls  { calls }                          // the Iterator "Done"
//   class Tool       { name, description, parameters: type }  // first-class `type`
//
// THE PROBLEM this file is about: with dozens of tools, shipping every full JSON
// Schema on EVERY turn dominates the prompt and defeats prompt caching (the
// cached prefix only hits if the leading tool block is byte-identical turn over
// turn). Two escape hatches exist in the wild:
//   * OpenAI Responses `tool_search` + `defer_loading: true` — the model pages
//     deferred definitions into context on demand. HOSTED search.
//   * Your own registry answering the model's search query per tenant/permission
//     set — CLIENT-EXECUTED search. The full catalog never ships; the model asks,
//     your code resolves.
//   * MCP — the cross-provider analog: tools discovered from a remote catalog at
//     connect time rather than declared inline.
//
// The teaching point: NONE of this fits in the plain `Tools` seam. `begin(prompt,
// tools)` takes a FLAT `Tool[]` and the proposal assumes the whole list ships.
// Deferral needs (1) per-tool loading policy, (2) namespacing metadata, and (3)
// a search-resolution hook for the client-executed mode. So we introduce a
// NET-NEW capability `SearchableTools requires Tools` plus three net-new types
// (`Namespace`, `Catalog`, a `tool_search`-shaped synthetic tool), and we show
// the real divergence: OpenAI does deferral on the wire; Anthropic/Gemini CANNOT,
// so a portable provider must emulate it client-side by pruning the list before
// `begin`.
// ============================================================================


// ----------------------------------------------------------------------------
// NET-NEW type #1 — a Tool gains a loading policy and a namespace.
//
// We do NOT touch the canonical `Tool` (name, description, parameters: type).
// Instead a `CatalogTool` WRAPS a Tool with the two facts deferral needs:
//   * defer:     should this definition be withheld until searched?
//   * namespace: the intent-group this tool belongs to (the namespacing
//                discipline the background calls load-bearing for search quality).
// Keeping `Tool` untouched means every plain-Tools provider still works; the
// extra policy is additive and only the SearchableTools impl reads it.
// ----------------------------------------------------------------------------
class CatalogTool {
  tool: Tool                         // the canonical Tool (name/description/parameters: type)
  defer: bool                        // defer_loading: hold the schema back until searched
  namespace: string                  // intent-group, e.g. "billing", "tickets"
}

// NET-NEW type #2 — a namespace: a short, DISCRIMINATIVE description plus its
// (mostly deferred) members. This is what the model searches over first; the
// per-tool schemas load only after a namespace is picked. Keep <~10 tools each.
class Namespace {
  name: string                       // "billing"
  description: string                // short, discriminative — the searchable surface
  tools: CatalogTool[]
}

// NET-NEW type #3 — the catalog: the full set of namespaces for a request, plus
// which provider-side search MODE to use. The catalog is the input the app hands
// the searchable loop INSTEAD of a flat Tool[].
class Catalog {
  namespaces: Namespace[]
  // "hosted"  -> ship everything (deferred-flagged) + a tool_search tool; the
  //              PROVIDER indexes and pages definitions in.
  // "client"  -> ship ONLY namespace descriptions + a search tool; YOUR registry
  //              resolves the model's query and returns matching definitions.
  mode: string
}

// Flatten helpers used by both the hosted and client paths.
class Catalog {
  // Every tool across every namespace (used by hosted mode to ship the lot).
  function all_tools(self) -> CatalogTool[] {
    let out: CatalogTool[] = [];
    for (let ns in self.namespaces) {
      for (let t in ns.tools) { out = out.append(t); }
    }
    out
  }
  // Only the non-deferred tools (always loaded, even in hosted/client mode).
  function eager_tools(self) -> CatalogTool[] {
    let out: CatalogTool[] = [];
    for (let t in self.all_tools()) {
      if (!t.defer) { out = out.append(t); }
    }
    out
  }
}


// ----------------------------------------------------------------------------
// NET-NEW capability — SearchableTools requires Tools.
//
// It REFINES Tools: it keeps the same begin/step/submit/run_tools seam (so the
// dispatch loop is unchanged), but `begin_searchable` takes a `Catalog` and a
// `SearchResolver` instead of a flat `Tool[]`. The provider decides whether the
// search is hosted (wire `tool_search` + `defer_loading`) or emulated client-side
// (prune the list + intercept the synthetic search tool).
//
// The resolver is the CLIENT-EXECUTED search hook: when the model issues a search
// for "refund issuance", the resolver returns the matching CatalogTool[] from
// YOUR registry. In hosted mode the provider never calls it.
// ----------------------------------------------------------------------------
interface SearchResolver {
  // Map a model-issued search query to the tools your registry actually exposes
  // for THIS request (tenant/permission set). Returns the definitions to load.
  function resolve(self, query: string) -> CatalogTool[] throws baml.errors.LlmClient
}

interface SearchableTools requires Tools {
  // Same Transcript associated type as Tools — searchable is a refinement, not a
  // new interaction shape. The provider's transcript additionally remembers which
  // deferred definitions are currently "loaded" so step/submit stay consistent.
  function begin_searchable<T>(self, prompt: baml.llm.PromptAst, catalog: Catalog, resolver: SearchResolver)
      -> Transcript throws Error

  // Default searchable loop: identical to Tools.run_tools EXCEPT it starts from a
  // Catalog and intercepts the synthetic search tool. The provider's step()
  // surfaces a normal ToolCalls; if a call's name is the reserved search tool, we
  // resolve it through the resolver and submit the loaded definitions back as a
  // ToolResult — WITHOUT running app dispatch for it. Real tool calls flow to
  // ctx.dispatch exactly as before.
  function search_tools<T>(self, prompt: baml.llm.PromptAst, catalog: Catalog,
                           resolver: SearchResolver, ctx: ExecutionContext) -> T throws Error {
    let t = self.begin_searchable<T>(prompt, catalog, resolver);
    while (true) {
      match (self.step<T>(t)) {
        let calls: ToolCalls => {
          // Split: the reserved search tool is handled HERE (resolve + reload);
          // everything else goes to the app's dispatch. In hosted mode the
          // provider answers search below the seam, so search calls never reach
          // this split — `to_search` is simply empty.
          let to_search: ToolCall[] = SearchableToolsOps.filter_search(calls.calls);
          let to_dispatch: ToolCall[] = SearchableToolsOps.filter_real(calls.calls);

          let results: ToolResult[] = [];
          for (let sc in to_search) {
            let query: string = SearchableToolsOps.search_query_of(sc.args);
            let loaded: CatalogTool[] = resolver.resolve(query);
            // The "result" of a search tool is the set of definitions that just
            // became callable. step()'s next turn will see these as loaded.
            results = results.append(ToolResult {
              id: sc.id,
              output: SearchableToolsOps.search_result_payload(loaded),
            });
          }
          // app dispatch for the genuine tool calls (correlated by opaque id)
          let dispatched: ToolResult[] = ctx.dispatch(to_dispatch);
          for (let d in dispatched) { results = results.append(d); }

          t = self.submit(t, results);
        },
        let value: T => { return value; },
      }
    }
    baml.sys.panic("unreachable")
  }
}

// Plain helpers, namespaced as statics so both providers reuse them.
class SearchableToolsOps {
  // The reserved tool name the search surfaces as on the wire (OpenAI's
  // tool_search call comes back under this synthetic name in client mode).
  function search_tool_name() -> string { "tool_search" }

  function filter_search(calls: ToolCall[]) -> ToolCall[] {
    let out: ToolCall[] = [];
    for (let c in calls) {
      if (c.name == SearchableToolsOps.search_tool_name()) { out = out.append(c); }
    }
    out
  }
  function filter_real(calls: ToolCall[]) -> ToolCall[] {
    let out: ToolCall[] = [];
    for (let c in calls) {
      if (c.name != SearchableToolsOps.search_tool_name()) { out = out.append(c); }
    }
    out
  }
  function search_query_of(args: map<string, unknown>) -> string throws baml.errors.LlmClient {
    // the search tool's single param is `{ "query": string }`
    $rust_io_function
  }
  // Serialize the loaded definitions into the payload the model receives as the
  // search result: name + description + full JSON Schema, now in context.
  function search_result_payload(loaded: CatalogTool[]) -> unknown throws baml.errors.LlmClient {
    $rust_io_function
  }
  // The synthetic search tool, declared to the model so it can issue searches.
  // Its `parameters` is a real first-class `type` ({ query: string }).
  function search_tool() -> Tool {
    Tool {
      name: SearchableToolsOps.search_tool_name(),
      description: "Search the tool catalog for tools relevant to the task, then call them.",
      parameters: reflect.type_of<SearchQuery>(),
    }
  }
}
class SearchQuery { query: string }


// ============================================================================
// PROVIDER A — OpenAI (Responses): HOSTED tool_search + defer_loading.
// ============================================================================
//
// OpenAI is the ONE provider with first-class deferral. On the wire we ship:
//   * { "type": "tool_search" }
//   * every eager tool inline
//   * every deferred tool with  "defer_loading": true
// and the PROVIDER indexes + pages them in. The model's search happens below the
// seam; our search_tools() split therefore sees `to_search` empty because the
// provider already resolved it and only surfaces REAL tool calls as ToolCalls.
//
// Cost story this encodes: the deferred schemas are NOT in the leading prompt,
// so the cacheable prefix (system + eager tools + tool_search) stays byte-stable
// across turns that don't page anything in => prompt-cache hits hold.
// ----------------------------------------------------------------------------
class OpenAI {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

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

  // Plain Tools (flat list) still works — searchable is additive.
  implements Tools {
    type Transcript = OpenAiTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> OpenAiTranscript throws baml.errors.LlmClient {
      OpenAiTranscript {
        model: self.model,
        api_key: self.api_key,
        input: baml.llm.prompt_to_responses_input(prompt),
        wire_tools: OpenAI.encode_tools_flat(tools),    // no deferral: ship them all eagerly
        loaded_names: [],                               // unused in flat mode
      }
    }
    function step<T>(self, t: OpenAiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      OpenAI.step_impl<T>(t)
    }
    function submit(self, t: OpenAiTranscript, results: ToolResult[]) -> OpenAiTranscript throws baml.errors.LlmClient {
      OpenAI.submit_impl(t, results)
    }
  }

  // HOSTED searchable: this is where defer_loading lives.
  implements SearchableTools {
    function begin_searchable<T>(self, prompt: baml.llm.PromptAst, catalog: Catalog, resolver: SearchResolver)
        -> OpenAiTranscript throws baml.errors.LlmClient {
      // resolver is IGNORED in hosted mode — OpenAI runs the search itself.
      // DIVERGENCE that matters: we encode each tool with a `defer_loading` flag
      // and prepend a { "type":"tool_search" } entry. Deferred schemas ARE on the
      // wire (OpenAI needs them to index) but they do NOT enter model context
      // until searched — that's the provider's job, not ours.
      OpenAiTranscript {
        model: self.model,
        api_key: self.api_key,
        input: baml.llm.prompt_to_responses_input(prompt),
        wire_tools: OpenAI.encode_tools_searchable(catalog),    // tool_search + defer_loading:true
        loaded_names: [],
      }
    }
  }

  // --- statics: the encoders & host bridges, all BELOW the seam ---

  // Flat: every tool inline, full JSON Schema. The expensive default.
  function encode_tools_flat(tools: Tool[]) -> string throws baml.errors.LlmClient {
    let entries: string[] = [];
    for (let tool in tools) {
      let schema: string = baml.reflect.type_to_json_schema(tool.parameters);
      entries = entries.append(baml.llm.openai_tool_entry(tool.name, tool.description, schema));
    }
    baml.llm.json_array(entries)
  }

  // Searchable: prepend { "type":"tool_search" }, then emit each CatalogTool with
  // its `defer_loading` flag set from CatalogTool.defer. The namespace name is
  // folded into the wire tool name so the model's search groups correctly
  // (namespacing discipline -> wire-visible prefix, e.g. "billing.process_refund").
  function encode_tools_searchable(catalog: Catalog) -> string throws baml.errors.LlmClient {
    let entries: string[] = [];
    entries = entries.append(baml.llm.openai_tool_search_entry());   // { "type": "tool_search" }
    for (let ns in catalog.namespaces) {
      for (let ct in ns.tools) {
        let qualified: string = OpenAI.qualify(ns.name, ct.tool.name);  // "billing.process_refund"
        let schema: string = baml.reflect.type_to_json_schema(ct.tool.parameters);
        entries = entries.append(
          baml.llm.openai_tool_entry_deferrable(qualified, ct.tool.description, schema, ct.defer)
        );
      }
    }
    baml.llm.json_array(entries)
  }
  // Namespacing discipline made wire-real: prefix the tool name with its group.
  function qualify(namespace: string, name: string) -> string { $rust_io_function }

  function step_impl<T>(t: OpenAiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
    let resp: baml.http.Response = OpenAI.post_responses(t) catch (e) {
      _ => { throw baml.errors.LlmClient { message: "openai responses failed" }; }
    };
    let body: string = resp.text();
    // In hosted mode a tool_search call is resolved by the provider; we only ever
    // see REAL function calls surface as output items. (If we DID surface a
    // tool_search item, search_tools()'s split handles it — but OpenAI doesn't.)
    if (baml.llm.responses_has_function_calls(body)) {
      let raw: OpenAiToolCall[] = baml.llm.responses_extract_function_calls(body);
      let calls: ToolCall[] = [];
      for (let r in raw) {
        // strip the namespace prefix back off so the app's handler name matches
        calls = calls.append(ToolCall {
          id: r.call_id,
          name: OpenAI.unqualify(r.name),
          args: baml.sap.parse<map<string, unknown>>(r.arguments_json),
        });
      }
      OpenAI.remember_assistant_turn(t, body);
      return ToolCalls { calls: calls };
    }
    baml.sap.parse<T>(baml.llm.responses_output_text(body))
  }
  function unqualify(qualified: string) -> string { $rust_io_function }

  function submit_impl(t: OpenAiTranscript, results: ToolResult[]) -> OpenAiTranscript throws baml.errors.LlmClient {
    let appended: string[] = [];
    for (let r in results) {
      appended = appended.append(
        baml.llm.responses_function_output(r.id, baml.sap.to_json(r.output))   // correlated by call_id
      );
    }
    OpenAI.append_input(t, appended)
  }

  function post_responses(t: OpenAiTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_assistant_turn(t: OpenAiTranscript, body: string) -> null { $rust_io_function }
  function append_input(t: OpenAiTranscript, items: string[]) -> OpenAiTranscript { $rust_io_function }
}

class OpenAiTranscript {
  model: string
  api_key: string
  input: string                      // serialized Responses input items (assistant turns preserved)
  wire_tools: string                 // tool_search + (deferred) function entries
  loaded_names: string[]             // unused for hosted; present for symmetry with emulated mode
}
class OpenAiToolCall { call_id: string, name: string, arguments_json: string }


// ============================================================================
// PROVIDER B — Anthropic (Messages): NO native deferral. Emulate CLIENT-SIDE.
// ============================================================================
//
// Anthropic (and Gemini) expect the COMPLETE tools array on every request. There
// is no defer_loading and no tool_search. So to get the token/cache win on these
// providers, the LIBRARY must emulate searchable tools entirely client-side:
//
//   * begin_searchable ships ONLY: the namespace descriptions + eager tools + a
//     SYNTHETIC `tool_search` tool we declare ourselves. Deferred schemas are
//     withheld — exactly the token saving, but achieved by US, not the wire.
//   * When the model calls our synthetic `tool_search`, step() surfaces it as a
//     NORMAL ToolCall named "tool_search". The default search_tools() loop
//     intercepts it, runs the resolver (CLIENT-EXECUTED search), and submits the
//     loaded definitions back as a tool_result.
//   * Here's the leak we must own: loading a definition means MUTATING the wire
//     tool list for the NEXT request (adding the now-searched schemas). That
//     CHANGES the leading tool block => the prompt-cache prefix is INVALIDATED on
//     the turn that loads tools. On OpenAI the deferred schemas never moved the
//     prefix; here every successful search costs a cache miss. We record loaded
//     names on the transcript so subsequent turns stay stable until the next load.
// ----------------------------------------------------------------------------
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int                    // bespoke, required by Messages

  implements ChatModelOptions { model as model, api_key as api_key }

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
    type Transcript = AnthropicTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      AnthropicTranscript {
        model: self.model,
        api_key: self.api_key,
        max_tokens: self.max_tokens,
        messages: baml.llm.prompt_to_anthropic_messages(prompt),
        wire_tools: Anthropic.encode_tools_anthropic(tools),
        catalog: Catalog { namespaces: [], mode: "client" },   // empty: flat mode
        loaded_names: [],
        pending_ids: [],
      }
    }
    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      Anthropic.step_impl<T>(t)
    }
    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      Anthropic.submit_impl(t, results)
    }
  }

  // EMULATED searchable: ship only descriptions + the synthetic search tool.
  implements SearchableTools {
    function begin_searchable<T>(self, prompt: baml.llm.PromptAst, catalog: Catalog, resolver: SearchResolver)
        -> AnthropicTranscript throws baml.errors.LlmClient {
      // The wire tools at START are: every EAGER tool + our synthetic tool_search.
      // Deferred schemas are withheld. The resolver is NOT called here — it's
      // called later, by the default loop, when the model issues a search.
      let eager: CatalogTool[] = catalog.eager_tools();
      let wire: string = Anthropic.encode_searchable_start(catalog, eager);
      AnthropicTranscript {
        model: self.model,
        api_key: self.api_key,
        max_tokens: self.max_tokens,
        messages: baml.llm.prompt_to_anthropic_messages(prompt),
        wire_tools: wire,
        catalog: catalog,            // kept so submit() can splice in newly-loaded schemas
        loaded_names: [],            // grows as searches resolve -> cache-prefix churn
        pending_ids: [],
      }
    }
  }

  // --- statics ---

  function encode_tools_anthropic(tools: Tool[]) -> string throws baml.errors.LlmClient {
    let entries: string[] = [];
    for (let tool in tools) {
      let schema: string = baml.reflect.type_to_json_schema(tool.parameters);
      entries = entries.append(baml.llm.anthropic_tool_entry(tool.name, tool.description, schema));
    }
    baml.llm.json_array(entries)
  }

  // Start state: eager tools (full schema) + a description-only stub per namespace
  // baked into the synthetic search tool's description, + the search tool itself.
  // The namespace descriptions are what the model "searches" over.
  function encode_searchable_start(catalog: Catalog, eager: CatalogTool[]) -> string throws baml.errors.LlmClient {
    let entries: string[] = [];
    for (let ct in eager) {
      let schema: string = baml.reflect.type_to_json_schema(ct.tool.parameters);
      let qualified: string = Anthropic.qualify(ct.namespace, ct.tool.name);
      entries = entries.append(baml.llm.anthropic_tool_entry(qualified, ct.tool.description, schema));
    }
    // synthetic search tool, with the namespace catalog folded into its description
    // (short discriminative descriptions only — NOT the deferred schemas).
    let search_desc: string = Anthropic.namespace_digest(catalog);
    entries = entries.append(
      baml.llm.anthropic_tool_entry("tool_search", search_desc,
        baml.reflect.type_to_json_schema(reflect.type_of<SearchQuery>()))
    );
    baml.llm.json_array(entries)
  }
  // Build the short, discriminative "here are the groups you can search" text.
  function namespace_digest(catalog: Catalog) -> string throws baml.errors.LlmClient { $rust_io_function }
  function qualify(namespace: string, name: string) -> string { $rust_io_function }

  function step_impl<T>(t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
    let resp: baml.http.Response = Anthropic.post_messages(t) catch (e) {
      _ => { throw baml.errors.LlmClient { message: "anthropic messages failed" }; }
    };
    let body: string = resp.text();
    if (baml.llm.anthropic_stop_reason(body) == "tool_use") {
      let raw: AnthropicToolUse[] = baml.llm.anthropic_extract_tool_use(body);
      let calls: ToolCall[] = [];
      let ids: string[] = [];
      for (let r in raw) {
        // The synthetic "tool_search" comes back like any other tool_use; we pass
        // its name THROUGH unqualified so the default loop's split recognizes it.
        calls = calls.append(ToolCall { id: r.id, name: Anthropic.unqualify(r.name), args: r.input });
        ids = ids.append(r.id);
      }
      Anthropic.remember_assistant_turn(t, body, ids);
      return ToolCalls { calls: calls };
    }
    baml.sap.parse<T>(baml.llm.anthropic_text(body))
  }
  function unqualify(qualified: string) -> string { $rust_io_function }

  function submit_impl(t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
    // THE EMULATION LEAK, concretely. If any result corresponds to a search
    // (its id was a tool_search call), the default loop has already turned the
    // resolver output into a ToolResult whose `output` is the loaded definitions.
    // We must:
    //   1. splice those newly-loaded schemas INTO t.wire_tools for the next turn
    //      (so the model can actually call them),
    //   2. record them in loaded_names,
    //   3. emit the tool_result blocks positionally (Anthropic's ordering rule).
    // Step (1) mutates the leading tool block => prompt cache prefix changes on
    // exactly the turns that load tools. That cost is unavoidable off OpenAI.
    let with_loaded: AnthropicTranscript = Anthropic.splice_loaded_schemas(t, results);
    let ordered: ToolResult[] = Anthropic.reorder_by_ids(results, with_loaded.pending_ids);
    let blocks: string[] = [];
    for (let r in ordered) {
      blocks = blocks.append(
        baml.llm.anthropic_tool_result_block(r.id, baml.sap.to_json(r.output))
      );
    }
    Anthropic.append_user_message(with_loaded, blocks)
  }
  // Read the search ToolResults, pull the loaded CatalogTools out, add their full
  // schemas to wire_tools, and grow loaded_names. The cache-busting step.
  function splice_loaded_schemas(t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
    $rust_io_function
  }
  function reorder_by_ids(results: ToolResult[], order: string[]) -> ToolResult[] {
    let out: ToolResult[] = [];
    for (let id in order) {
      for (let r in results) { if (r.id == id) { out = out.append(r); } }
    }
    out
  }
  function post_messages(t: AnthropicTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_assistant_turn(t: AnthropicTranscript, body: string, ids: string[]) -> null { $rust_io_function }
  function append_user_message(t: AnthropicTranscript, blocks: string[]) -> AnthropicTranscript { $rust_io_function }
}

class AnthropicTranscript {
  model: string
  api_key: string?
  max_tokens: int
  messages: string
  wire_tools: string                 // grows as searches load definitions (cache churn)
  catalog: Catalog                   // kept so submit() can splice deferred schemas
  loaded_names: string[]             // which deferred tools are now callable
  pending_ids: string[]              // tool_use id order for positional re-assembly
}
class AnthropicToolUse { id: string, name: string, input: map<string, unknown> }


// ============================================================================
// PROVIDER C — McpProvider: MCP as the cross-provider analog of deferred tools.
// ============================================================================
//
// MCP is the other way to avoid shipping a giant static list: tools are
// discovered from a remote server at connect time, not declared inline. This
// provider wraps an INNER chat provider (any Tools-capable one) and federates the
// catalog to MCP servers. The model never sees the full registry; the MCP layer
// resolves discovery.
//
// Crucially this maps onto the SAME SearchableTools seam: MCP's `tools/list` (with
// optional filtering) IS a search resolver, and `tools/call` IS dispatch. So an
// MCP catalog is just a Catalog whose resolver federates to servers. We get the
// cross-provider story for free: hand McpProvider an inner OpenAI (hosted page-in)
// OR an inner Anthropic (client emulation) and the discovery story is identical
// above the seam.
// ----------------------------------------------------------------------------
class McpProvider {
  inner: Provider                    // the actual chat model (OpenAI/Anthropic/...)
  servers: McpServer[]               // remote tool catalogs

  // McpProvider is itself a Provider: plain .call just delegates (no tools).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
    }
  }

  // Tools: delegate the loop to the inner provider, but the catalog is discovered.
  implements Tools {
    type Transcript = McpTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> McpTranscript throws baml.errors.LlmClient {
      // Even the flat form federates: union the inline `tools` with whatever the
      // servers advertise at connect (tools/list). This is discovery-at-connect.
      let discovered: Tool[] = McpProvider.list_all(self.servers);
      let all: Tool[] = McpProvider.union_tools(tools, discovered);
      match (self.inner) {
        let tp: Tools => {
          let inner = tp.begin<T>(prompt, all);
          return McpTranscript { owner: tp, inner: inner, servers: self.servers };
        },
        _ => { throw baml.errors.LlmClient { message: "inner provider is not tool-capable" }; }
      }
    }
    function step<T>(self, t: McpTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      t.owner.step<T>(t.inner)
    }
    function submit(self, t: McpTranscript, results: ToolResult[]) -> McpTranscript throws baml.errors.LlmClient {
      let next = t.owner.submit(t.inner, results);
      McpTranscript { owner: t.owner, inner: next, servers: t.servers }
    }
  }

  // SearchableTools: MCP's `tools/list` filtering IS the search. We expose the
  // servers as a SearchResolver and let the inner provider's searchable loop drive
  // it. If the inner provider has NATIVE deferral (OpenAI), we hand it a catalog
  // built from a SHALLOW listing (names + descriptions only) and let it page in
  // full schemas via tools/list-on-demand; if not (Anthropic), the client
  // emulation does the same with our resolver.
  implements SearchableTools {
    function begin_searchable<T>(self, prompt: baml.llm.PromptAst, catalog: Catalog, resolver: SearchResolver)
        -> McpTranscript throws baml.errors.LlmClient {
      // Compose resolvers: the app's `resolver` first, then federate to MCP
      // servers for anything not in the local registry. (Two registries reconciled
      // — the exact drift hazard the background flags.)
      let federated: SearchResolver = McpResolver { servers: self.servers, fallback: resolver };
      match (self.inner) {
        let st: SearchableTools => {
          let inner = st.begin_searchable<T>(prompt, catalog, federated);
          return McpTranscript { owner: self.inner_as_tools(), inner: inner, servers: self.servers };
        },
        // inner can't do searchable at all -> degrade to flat federated discovery.
        _ => {
          let discovered: Tool[] = McpProvider.list_all(self.servers);
          match (self.inner) {
            let tp: Tools => {
              let inner = tp.begin<T>(prompt, discovered);
              return McpTranscript { owner: tp, inner: inner, servers: self.servers };
            },
            _ => { throw baml.errors.LlmClient { message: "inner provider is not tool-capable" }; }
          }
        }
      }
    }
  }

  function inner_as_tools(self) -> Tools throws baml.errors.LlmClient {
    match (self.inner) {
      let tp: Tools => tp,
      _ => { throw baml.errors.LlmClient { message: "inner provider is not tool-capable" }; }
    }
  }

  // host bridges to MCP servers
  function list_all(servers: McpServer[]) -> Tool[] throws baml.errors.LlmClient { $rust_io_function }
  function union_tools(a: Tool[], b: Tool[]) -> Tool[] { $rust_io_function }
}

class McpServer {
  name: string                       // namespace: each server is naturally one namespace
  url: string
  auth: string?
}
class McpTranscript {
  owner: Tools
  inner: unknown
  servers: McpServer[]
}

// MCP-backed search resolver: answer the model's query by filtering tools/list on
// the federated servers, falling back to the app's local resolver. This is the
// "client-executed search must reconcile two registries" case made concrete.
class McpResolver {
  servers: McpServer[]
  fallback: SearchResolver

  implements SearchResolver {
    function resolve(self, query: string) -> CatalogTool[] throws baml.errors.LlmClient {
      // 1. ask each MCP server for matching tools (tools/list with a filter)
      let remote: CatalogTool[] = McpResolver.search_servers(self.servers, query) catch (e) {
        _ => { []; }     // a dead server must not abort the whole search
      };
      // 2. union with whatever the app's local registry resolves
      let local: CatalogTool[] = self.fallback.resolve(query);
      McpResolver.dedup(remote, local)   // drift reconciliation: prefer one source on name clash
    }
  }
  function search_servers(servers: McpServer[], query: string) -> CatalogTool[] throws baml.errors.LlmClient { $rust_io_function }
  function dedup(remote: CatalogTool[], local: CatalogTool[]) -> CatalogTool[] { $rust_io_function }
}


// ============================================================================
// A concrete client-side SearchResolver over an in-process registry.
// ============================================================================
//
// The app's own registry: tools that exist for THIS tenant/permission set. The
// resolver matches the model's query against namespace + description (the
// background's "your registry resolves the model query"). Revoked tools simply
// aren't returned — drift surfaces as an empty search rather than a phantom call.
// ----------------------------------------------------------------------------
class RegistryResolver {
  catalog: Catalog                   // the full set this tenant may use
  allowed: string[]                  // permission filter: tool names this principal may load

  implements SearchResolver {
    function resolve(self, query: string) -> CatalogTool[] throws baml.errors.LlmClient {
      // rank namespaces + tools by relevance to `query`, then filter by permission.
      let ranked: CatalogTool[] = RegistryResolver.rank(self.catalog, query);
      let out: CatalogTool[] = [];
      for (let ct in ranked) {
        if (self.allowed.contains(ct.tool.name)) { out = out.append(ct); }
      }
      out
    }
  }
  // host bridge: semantic/lexical ranking of the catalog against the query.
  function rank(catalog: Catalog, query: string) -> CatalogTool[] throws baml.errors.LlmClient { $rust_io_function }
}
