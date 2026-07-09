// scenario 13 — searchable tools: ../../../../../llm-provider/ideas/scenarios/13-searchable-tools/implement.baml
//   implementation.baml — LIBRARY / std-lib-author code.
//
// The design's teaching point (background 02 "Scaling the tool catalog"): when an
// agent has dozens of tools, shipping every JSON Schema on every turn dominates
// the prompt and defeats prompt caching. The escape hatch is DEFERRED / SEARCHABLE
// catalogs — search and pagination become TOOLS THEMSELVES (the meta-tool
// pattern), and dispatch is CATALOG-BACKED: the model calls a synthetic
// `tool_search`, the app resolves the query against its own registry, and the
// matching tool definitions page in on demand.
//
// The design lifted all of this into NET-NEW surface: `SearchableTools requires
// Tools` (with `begin_searchable` + a default `search_tools` loop), a namespaced
// `Catalog` with per-tool `defer` flags, and a `SearchResolver` hook — plus two
// providers (OpenAI hosted `tool_search`/`defer_loading`, Anthropic emulated
// client-side) and an `McpProvider` federating discovery to remote servers.
//
// On the REAL surface NONE of that net-new capability exists (stdlib is frozen),
// and it turns out it doesn't NEED to: the meta-tool pattern collapses ENTIRELY
// onto the existing `baml.ai.Tools` loop, ABOVE the seam.
//   - `tool_search` is a plain CLIENT-EXECUTED `baml.ai.Tool` (`Tool.from_type`),
//     no different from any other tool the loop dispatches.
//   - The `SearchResolver` is just the app's DISPATCHER logic: when the loop hands
//     `tool_search` to dispatch, the app resolves the query against its Catalog and
//     returns the matching definitions as an ordinary `ToolResult` (the "these are
//     now loaded" payload the model reads next turn). This IS the design's
//     "client-executed search" / emulated-deferral path — the only honest one here.
//   - Deferral = only advertising eager tools + `tool_search` up front; the
//     deferred definitions live behind the search and page in through its result.
//   - `baml.json.path_or` reads the model's (noisy) search query leniently;
//     `baml.ai.invoke_tool<A>` SAP-coerces a real tool's args into its typed shape.
//
// DIVERGENCES (net-new surface that cannot be added under the stdlib freeze — see
// 13.json):
//   * PROVIDER-NATIVE hosted deferral (OpenAI `tool_search` + `defer_loading` on
//     the WIRE) is BLOCKED: there is no such wire feature on `baml.ai.OpenAi`, AND
//     no `Tools.set_tools` seam to splice a newly-loaded schema into the wire tool
//     list MID-LOOP (the D7 gap noted in scenario 10). So deferral is EMULATED
//     client-side only — exactly the design's Anthropic path.
//   * `McpProvider` federation is BLOCKED: no MCP host surface (P8) to discover a
//     remote catalog or reconcile two registries.
//   * The prompt-cache ECONOMICS divergence the evaluation is really about (OpenAI
//     native page-in keeps a byte-stable cacheable prefix; Anthropic emulation
//     busts the cache on the turns that load tools) is a BELOW-seam concern and is
//     not observable on this surface at all.


// ── NET-NEW type #1: the design's `CatalogTool` — a Tool + a loading policy + a
// namespace. The canonical `baml.ai.Tool` is left UNTOUCHED (so every plain-Tools
// provider still works); deferral policy + namespacing discipline wrap around it. ─
class s13_CatalogTool {
    tool: baml.ai.Tool,
    /// Withhold the schema until searched. `false` ⇒ eager (always advertised).
    deferred: bool,
    /// Intent-group, e.g. "billing" / "tickets" — the searchable surface.
    namespace: string,
}

/// NET-NEW type #2: a namespace — a short, DISCRIMINATIVE description plus its
/// (mostly deferred) members. This is what the model searches over first.
class s13_Namespace {
    name: string,
    description: string,
    tools: s13_CatalogTool[],
}

/// NET-NEW type #3: the catalog handed to the searchable loop INSTEAD of a flat
/// `Tool[]`. No `mode` field: the real surface only supports the client-executed
/// (emulated) path, so there is no "hosted" mode to select.
class s13_Catalog {
    namespaces: s13_Namespace[],

    /// Every tool across every namespace.
    function all_tools(self) -> s13_CatalogTool[] throws never {
        let out: s13_CatalogTool[] = [];
        for (let ns in self.namespaces) {
            for (let t in ns.tools) {
                out.push(t);
            }
        }
        out
    }

    /// Only the non-deferred tools — always advertised, even before any search.
    function eager_tools(self) -> baml.ai.Tool[] throws never {
        let out: baml.ai.Tool[] = [];
        for (let t in self.all_tools()) {
            if (!t.deferred) {
                out.push(t.tool);
            }
        }
        out
    }
}


// ── The client-executed search (the design's `SearchResolver.resolve`) ────────
// Map the model's query to the tools it should page in. Matches on tool name,
// description, or namespace — the discriminative surface the background says search
// quality lives on. `throws never`: a pure registry lookup, no wire, no failure.
function s13_search_catalog(cat: s13_Catalog, query: string) -> s13_CatalogTool[] throws never {
    let q = query.to_lower_case();
    cat
        .all_tools()
        .filter(
            (t: s13_CatalogTool) -> bool {
                t.tool.name.to_lower_case().includes(q)
                    || t.tool.description.to_lower_case().includes(q)
                    || t.namespace.to_lower_case().includes(q)
            },
        )
}


// ── The `tool_search` META-TOOL: search IS a tool the model calls ─────────────
// Deferred tools are not advertised up front; the model discovers them by calling
// this. Its `parameters` is a real first-class `type` ({ query: string }).
class s13_SearchQuery {
    query: string,
}

function s13_search_tool() -> baml.ai.Tool throws never {
    s13_tool(
        "tool_search",
        "Search the tool catalog for tools relevant to the task, then call them.",
        reflect.type_of<s13_SearchQuery>(),
    )
}

/// `Tool.from_type` throws `Unsupported`; fold that into a placeholder so tool
/// factories are ordinary (throws-free).
function s13_tool(name: string, description: string, params: type) -> baml.ai.Tool throws never {
    baml.ai.Tool.from_type(name, description, params) catch (e) {
        _ => baml.ai.Tool { name: name, description: description, parameters: "{}" },
    }
}

/// The tools advertised to the model at the START of a run: every EAGER tool plus
/// the `tool_search` meta-tool. Deferred definitions are WITHHELD — that is the
/// token saving, achieved by US (emulated), not the wire.
function s13_advertised_tools(cat: s13_Catalog) -> baml.ai.Tool[] throws never {
    let out = cat.eager_tools();
    out.push(s13_search_tool());
    out
}


// ── CATALOG-BACKED DISPATCH: the meta-tool pattern made real on the Tools seam ─
// A dispatch closure `(ToolCall[]) -> ToolResult[]` (the shape `ToolLoop`/
// `run_tools` take). It intercepts `tool_search` HERE — resolves the query against
// the app's catalog (client-executed search) and returns the matching definitions
// as the ToolResult (the "these are now loaded" payload). Every OTHER call is a
// real tool routed to the app's `handler`. Names paged in are recorded in `loaded`
// (injected by capture) so a test can prove the resolver ran. `throws never`.
function s13_catalog_dispatch(
    cat: s13_Catalog,
    loaded: string[],
    handler: (baml.ai.ToolCall) -> baml.ai.ToolResult throws never,
) -> ((baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never) {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        let out: baml.ai.ToolResult[] = [];
        for (let c in calls) {
            if (c.name == "tool_search") {
                // Lenient read of the model's (noisy wire) query.
                let query = baml.json.path_or(
                    baml.json.parse(c.args) catch (e) {
                        _ => null,
                    },
                    ".query",
                    "",
                );
                let hits = s13_search_catalog(cat, query);
                let names: string[] = [];
                for (let h in hits) {
                    names.push(h.namespace + "." + h.tool.name);
                    loaded.push(h.tool.name);
                }
                out.push(baml.ai.ToolResult {
                    id: c.id,
                    output: "{\"loaded\": [\"" + names.join("\",\"") + "\"]}",
                });
            } else {
                out.push(handler(c));
            }
        }
        out
    }
}


// ── An offline, deterministic Tools provider that scripts a SEQUENCE of turns ──
// scenario 10's `ScriptedTools` only ever emits a single "lookup" call, so it
// can't script "search, THEN call the tool you found, THEN answer". This scripts a
// batch of tool calls per turn, in order, then produces `final_json`.
class s13_ScriptTurn {
    calls: baml.ai.ToolCall[],
}

class s13_ScriptCursor {
    turn: int,
}

class s13_SearchScriptedTools {
    turns: s13_ScriptTurn[],
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = s13_ScriptCursor

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s13_ScriptCursor throws never {
            s13_ScriptCursor { turn: 0 }
        }
        function step<T>(self, t: s13_ScriptCursor) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.turn < self.turns.length()) {
                return baml.ai.ToolCalls { calls: self.turns[t.turn].calls };
            }
            baml.sap.parse<T>(self.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s13 final parse failed"] },
            }
        }
        function submit(self, t: s13_ScriptCursor, results: baml.ai.ToolResult[]) -> s13_ScriptCursor throws never {
            s13_ScriptCursor { turn: t.turn + 1 }
        }
    }
}
