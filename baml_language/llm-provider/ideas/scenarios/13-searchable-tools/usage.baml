// ============================================================================
// 13 — Scaling the catalog: deferred & searchable tools  ::  usage.baml
//      (APPLICATION AUTHOR code)
// ============================================================================
//
// The promise: the app author declares a CATALOG of tools grouped into small,
// discriminative namespaces, marks the expensive ones `defer: true`, and calls
// the `.search_tools` companion. Whether the provider does this on the wire
// (OpenAI `tool_search`) or the library emulates it client-side (Anthropic) is
// invisible here. The ONE place portability leaks is cost, not code (see eval).
// ============================================================================


// --- Tool argument shapes: ordinary BAML classes -> first-class `type`. ---
class LookupOrderArgs   { order_id: string }
class ProcessRefundArgs { order_id: string, line_id: string, amount_cents: int, reason: string }
class SearchDocsArgs    { query: string }
class CreateTicketArgs  { subject: string, body: string, priority: string }
class EscalateArgs      { order_id: string, tier: string }


// ----------------------------------------------------------------------------
// THE CATALOG — grouped into small namespaces (the namespacing discipline).
// Each namespace gets a SHORT, DISCRIMINATIVE description: just enough for the
// model to pick the group. Detail lives in the deferred per-tool definitions.
// `defer: true` is the per-tool loading policy: cheap, common tools stay eager;
// expensive/rare ones load only when searched.
// ----------------------------------------------------------------------------
function SupportCatalog() -> Catalog {
  Catalog {
    mode: "hosted",     // OpenAI will index + page these; see HostedClient
    namespaces: [
      Namespace {
        name: "billing",
        description: "Look up orders and issue refunds for charges.",
        tools: [
          CatalogTool {
            namespace: "billing",
            defer: false,                                  // cheap + common -> always loaded
            tool: Tool {
              name: "lookup_order",
              description: "Look up an order by id.",
              parameters: reflect.type_of<LookupOrderArgs>(),
            },
          },
          CatalogTool {
            namespace: "billing",
            defer: true,                                   // expensive schema -> load on search
            tool: Tool {
              name: "process_refund",
              description: "Issue a refund for an order line.",
              parameters: reflect.type_of<ProcessRefundArgs>(),
            },
          },
        ],
      },
      Namespace {
        name: "tickets",
        description: "Search documentation and open or escalate support tickets.",
        tools: [
          CatalogTool {
            namespace: "tickets", defer: true,
            tool: Tool {
              name: "search_docs",
              description: "Full-text search the support knowledge base.",
              parameters: reflect.type_of<SearchDocsArgs>(),
            },
          },
          CatalogTool {
            namespace: "tickets", defer: true,
            tool: Tool {
              name: "create_ticket",
              description: "Open a new support ticket.",
              parameters: reflect.type_of<CreateTicketArgs>(),
            },
          },
          CatalogTool {
            namespace: "tickets", defer: true,
            tool: Tool {
              name: "escalate",
              description: "Escalate an order to a higher support tier.",
              parameters: reflect.type_of<EscalateArgs>(),
            },
          },
        ],
      },
      // ... dozens more namespaces, all deferred ...
    ],
  }
}


// ----------------------------------------------------------------------------
// CLIENTS — config only, sugar for `function Name() -> Provider`.
// ----------------------------------------------------------------------------

// HOSTED search: OpenAI Responses runs tool_search + defer_loading on the wire.
client HostedClient() {
  OpenAI { model: "gpt-5.1", api_key: env.OPENAI_API_KEY }
}

// EMULATED search: Anthropic has no native deferral, so the SAME .search_tools
// companion drives the library's client-side emulation. App code is identical.
client EmulatedClient() {
  Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 2048 }
}

// MCP-backed: discovery federates to remote servers; the inner provider can be
// either of the above. Here we wrap Anthropic (so MCP discovery + client
// emulation stack). Each server is naturally one namespace.
client McpClient() {
  McpProvider {
    inner: Anthropic { model: "claude-sonnet-4-6", api_key: env.ANTHROPIC_API_KEY, max_tokens: 2048 },
    servers: [
      McpServer { name: "billing", url: env.BILLING_MCP_URL, auth: env.BILLING_MCP_TOKEN },
      McpServer { name: "tickets", url: env.TICKETS_MCP_URL, auth: null },
    ],
  }
}


// ----------------------------------------------------------------------------
// THE RESOLVER — client-executed search over the app's own registry.
//
// Only used in `mode: "client"` (and MCP). The resolver answers the model's
// search query with the tools THIS tenant/permission set actually exposes.
// Hosted mode ignores it entirely — but we build it once and pass it everywhere,
// so swapping a client never changes the call site.
// ----------------------------------------------------------------------------
function SupportResolver(allowed: string[]) -> SearchResolver {
  RegistryResolver { catalog: SupportCatalog(), allowed: allowed }
}


// ----------------------------------------------------------------------------
// THE FUNCTION + the `.search_tools` companion.
//
// `Support(req)` is the plain `.call` form (no tools). `Support.search_tools(req,
// catalog, resolver, ctx)` is the capability companion. It `match`es the client's
// provider for `SearchableTools`; if the provider can't do searchable AT ALL it
// can still degrade to plain `.run_tools` over the flattened catalog (a real
// fallback exists here, unlike Realtime), or error if it isn't even Tools-capable.
// ----------------------------------------------------------------------------
function Support(request: string) -> string {
  client: HostedClient()
  prompt #"Resolve this support request: {{ request }}. Search for tools as needed."#
}
// Generated companion (shown for clarity; normally compiler-synthesized):
//
//   function Support$search_tools(request: string, catalog: Catalog,
//                                 resolver: SearchResolver, ctx: ExecutionContext) -> string {
//     let p = HostedClient();
//     match (p) {
//       let st: SearchableTools => st.search_tools<string>(render_prompt(request), catalog, resolver, ctx),
//       let tp: Tools           => tp.run_tools<string>(render_prompt(request),
//                                                       flatten_catalog(catalog), ctx),  // degrade: ship it all
//       _                       => throw baml.errors.Unsupported { message: "client cannot run tools" },
//     }
//   }
//
//   call site:  Support.search_tools("Refund order 4821", SupportCatalog(), resolver, ctx)


// ----------------------------------------------------------------------------
// Host-side entry points an app actually writes. Same body, three clients —
// hosted / emulated / MCP — and the divergence stays under the seam.
// ----------------------------------------------------------------------------

// HOSTED: catalog.mode == "hosted"; resolver passed but ignored by OpenAI.
function RunSupportHosted(request: string, principal_tools: string[]) -> string throws baml.errors.LlmClient {
  let ctx = ExecutionContext {
    function_name: "Support",
    args: { "request": request },
    deps: SupportDeps { tenant: "acme", region: "us" },
  };
  Support.search_tools(request, SupportCatalog(), SupportResolver(principal_tools), ctx)
}

// EMULATED: same call, but a client whose provider can't defer on the wire. The
// library prunes the list + intercepts the synthetic tool_search; the resolver
// (now LIVE) resolves the query from the app registry. Notice: app code unchanged.
function RunSupportEmulated(request: string, principal_tools: string[]) -> string throws baml.errors.LlmClient {
  let ctx = ExecutionContext {
    function_name: "Support",
    args: { "request": request },
    deps: SupportDeps { tenant: "acme", region: "us" },
  };
  let resolver = SupportResolver(principal_tools);
  // Bind a client-mode catalog so the emulation withholds deferred schemas.
  let catalog = ClientCatalog();
  // Drive against the emulated client explicitly (the function above is bound to
  // HostedClient; here we want EmulatedClient, so we open the seam by hand).
  let p = EmulatedClient();
  match (p) {
    let st: SearchableTools => st.search_tools<string>(render_prompt(request), catalog, resolver, ctx),
    _ => throw baml.errors.Unsupported { message: "client cannot run searchable tools" },
  }
}

// MCP: discovery federates to servers; the resolver also reconciles with the app
// registry inside McpResolver. Two registries, one search.
function RunSupportMcp(request: string, principal_tools: string[]) -> string throws baml.errors.LlmClient {
  let ctx = ExecutionContext {
    function_name: "Support",
    args: { "request": request },
    deps: SupportDeps { tenant: "acme", region: "us" },
  };
  let p = McpClient();
  match (p) {
    let st: SearchableTools =>
      st.search_tools<string>(render_prompt(request), ClientCatalog(), SupportResolver(principal_tools), ctx),
    _ => throw baml.errors.Unsupported { message: "client cannot run searchable tools" },
  }
}

// Same SupportCatalog, but mode flipped to client so deferred schemas are
// withheld and the resolver is actually invoked.
function ClientCatalog() -> Catalog {
  let base = SupportCatalog();
  Catalog { namespaces: base.namespaces, mode: "client" }
}

class SupportDeps { tenant: string, region: string }


// ----------------------------------------------------------------------------
// Custom dispatch policy WITHOUT touching deferral/threading: drive the loop by
// hand to add a per-load budget. begin_searchable/step/submit (and ALL search +
// cache logic) are untouched — this changes POLICY only, exactly like the plain
// tool loop. Here we cap how many searches the model may run (discovery latency
// guard: each search is an extra round-trip the background warns about).
// ----------------------------------------------------------------------------
function BudgetedSupport(request: string, max_searches: int,
                         resolver: SearchResolver, ctx: ExecutionContext) -> string throws baml.errors.LlmClient {
  let p = EmulatedClient();
  match (p) {
    let st: SearchableTools => {
      let t = st.begin_searchable<string>(render_prompt(request), ClientCatalog(), resolver);
      let searches: int = 0;
      while (true) {
        match (st.step<string>(t)) {
          let calls: ToolCalls => {
            let results: ToolResult[] = [];
            for (let c in calls.calls) {
              if (c.name == "tool_search") {
                if (searches >= max_searches) {
                  // refuse further discovery; tell the model to use what it has.
                  results = results.append(ToolResult {
                    id: c.id, output: { "error": "search budget exhausted; use loaded tools" },
                  });
                } else {
                  searches = searches + 1;
                  let loaded = resolver.resolve(search_query(c.args));
                  results = results.append(ToolResult { id: c.id, output: loaded });
                }
              } else {
                // a real tool call -> app dispatch (one entry)
                for (let r in ctx.dispatch([c])) { results = results.append(r); }
              }
            }
            t = st.submit(t, results);
          },
          let value: string => { return value; },
        }
      }
      baml.sys.panic("unreachable")
    },
    _ => throw baml.errors.Unsupported { message: "client cannot run searchable tools" },
  }
}
function search_query(args: map<string, unknown>) -> string throws baml.errors.LlmClient { $rust_io_function }
