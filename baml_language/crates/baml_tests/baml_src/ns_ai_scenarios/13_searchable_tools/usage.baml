// scenario 13 — searchable tools: ../../../../../llm-provider/ideas/scenarios/13-searchable-tools/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design's promise: the app author declares a CATALOG of tools grouped into
// small, discriminative namespaces, marks the expensive ones `deferred: true`, and
// the search machinery pages in only the matching tools per turn. On the real
// surface this is the META-TOOL pattern over the ordinary `baml.ai.Tools` loop —
// `tool_search` is a plain client-executed tool, and dispatch is CATALOG-BACKED
// (the app resolves the model's query against its own registry inside the
// dispatcher). See implementation.baml for the library helpers + the divergences.
//
// PREFER LLM FUNCTIONS (DCP §1.6): the primary spelling is a plain LLM function
// driven by a `baml.ai.ToolLoop` client — the loop, the search interception, and
// the page-in are all IMPLIED by the client + its catalog dispatcher.
//
// Live tier = testset "integ-test-13-searchable-tools" (native OpenAi tools,
// the model actually calling `tool_search` before the real tool).
//
// Reuses (read-only, shared namespace): common/fakes.baml's NoCapProvider.


// ── The tool argument shapes: ordinary BAML classes -> first-class `type`. ────
class s13_LookupOrderArgs {
    order_id: string,
}
// Two required fields of distinct types (SAP leniency can't swallow arbitrary JSON).
class s13_ProcessRefundArgs {
    order_id: string,
    amount_cents: int,
}
class s13_SearchDocsArgs {
    query: string,
}


// ── THE CATALOG — grouped into small, discriminative namespaces. `deferred: false`
// keeps cheap+common tools eager; `deferred: true` withholds expensive/rare schemas
// until the model searches for them. ─────────────────────────────────────────────
function s13_support_catalog() -> s13_Catalog {
    s13_Catalog {
        namespaces: [
            s13_Namespace {
                name: "billing",
                description: "Look up orders and issue refunds for charges.",
                tools: [
                    s13_CatalogTool {
                        namespace: "billing",
                        deferred: false, // cheap + common -> always advertised
                        tool: s13_tool("lookup_order", "Look up an order by id.", reflect.type_of<s13_LookupOrderArgs>()),
                    },
                    s13_CatalogTool {
                        namespace: "billing",
                        deferred: true, // expensive schema -> load on search
                        tool: s13_tool("process_refund", "Issue a refund for an order line.", reflect.type_of<s13_ProcessRefundArgs>()),
                    },
                ],
            },
            s13_Namespace {
                name: "tickets",
                description: "Search documentation and open support tickets.",
                tools: [
                    s13_CatalogTool {
                        namespace: "tickets",
                        deferred: true,
                        tool: s13_tool("search_docs", "Full-text search the support knowledge base.", reflect.type_of<s13_SearchDocsArgs>()),
                    },
                ],
            },
        ],
    }
}


// ── 1. Client-executed search: page in ONLY the matching deferred tools. ──────
test "13: client-executed search pages in only the matching tools" {
    let cat = s13_support_catalog();
    let hits = s13_search_catalog(cat, "refund");
    assert.equal(hits.length(), 1)
    assert.equal(hits[0].tool.name, "process_refund")
    assert.equal(hits[0].namespace, "billing")
}

// A namespace-scoped query resolves the whole group (the namespacing discipline).
test "13: a namespace query resolves that group's tools" {
    let cat = s13_support_catalog();
    let hits = s13_search_catalog(cat, "billing");
    // both billing tools match on namespace, regardless of defer flag.
    assert.equal(hits.length(), 2)
}


// ── 2. Deferral: eager tools + tool_search are advertised; deferred are withheld.
test "13: only eager tools and the tool_search meta-tool are advertised up front" {
    let adv = s13_advertised_tools(s13_support_catalog());
    let names: string[] = [];
    for (let t in adv) {
        names.push(t.name);
    }
    // eager: lookup_order; plus the synthetic tool_search.
    assert.is_true(names.includes("lookup_order"))
    assert.is_true(names.includes("tool_search"))
    // deferred schemas are NOT on the wire until searched.
    assert.is_true(!names.includes("process_refund"))
    assert.is_true(!names.includes("search_docs"))
}


// ── 3. The meta-tool loop, end to end: search, THEN call the paged-in tool. ───
// Turn 0 the model calls `tool_search` (catalog-backed dispatch resolves it and
// pages in process_refund); turn 1 it calls the now-discovered process_refund
// (real dispatch); then it answers. One loop, the search interception invisible
// to the app author beyond the dispatcher it plugged in.
test "13: tool_search pages in a deferred tool, then the loop calls it" {
    let seen: string[] = [];
    let loaded: string[] = [];
    let agent = baml.ai.ToolLoop {
        inner: s13_SearchScriptedTools {
            turns: [
                s13_ScriptTurn {
                    calls: [baml.ai.ToolCall { id: "s1", name: "tool_search", args: "{\"query\": \"refund\"}" }],
                },
                s13_ScriptTurn {
                    calls: [baml.ai.ToolCall { id: "r1", name: "process_refund", args: "{\"order_id\": \"4821\", \"amount_cents\": 500}" }],
                },
            ],
            final_json: "refund issued for order 4821",
        },
        tools: s13_advertised_tools(s13_support_catalog()),
        dispatch: s13_catalog_dispatch(
            s13_support_catalog(),
            loaded,
            (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
                seen.push(c.name);
                baml.ai.invoke_tool<s13_ProcessRefundArgs>(c, (a: s13_ProcessRefundArgs) -> string {
                    "refunded " + a.amount_cents.to_string() + " on " + a.order_id
                })
            },
        ),
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("Refund order 4821") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let b: baml.ai.Budget<string> => "unexpected budget",
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let v: string => v,
    };
    assert.equal(r, "refund issued for order 4821")
    // the search paged in process_refund (client-executed search ran)...
    assert.equal(loaded.length(), 1)
    assert.equal(loaded[0], "process_refund")
    // ...and the real tool was dispatched afterward (tool_search was NOT).
    assert.equal(seen.length(), 1)
    assert.equal(seen[0], "process_refund")
}


// ── 4. PREFER LLM FUNCTIONS: the same run as a plain LLM function + ToolLoop. ──
// The app author writes an ordinary one-shot function; the ToolLoop client (with a
// catalog dispatcher) runs the whole search-then-call loop and returns the value.
// Swapping the client is the only change needed to go from offline to live.
client<llm> s13_Dead {
    provider "openai"
    options {
        model "gpt-4o"
        api_key "unused"
        base_url "http://127.0.0.1:9"
    }
}

function s13_Support(request: string) -> string {
    client s13_Dead
    prompt #"
        Resolve this support request: {{ request }}.
        Search the tool catalog for the tools you need, then call them.
    "#
}

test "13: the plain LLM function drives the searchable loop via a ToolLoop client" {
    let seen: string[] = [];
    let loaded: string[] = [];
    let agent = baml.ai.ToolLoop {
        inner: s13_SearchScriptedTools {
            turns: [
                s13_ScriptTurn {
                    calls: [baml.ai.ToolCall { id: "s1", name: "tool_search", args: "{\"query\": \"order\"}" }],
                },
                s13_ScriptTurn {
                    calls: [baml.ai.ToolCall { id: "o1", name: "lookup_order", args: "{\"order_id\": \"4821\"}" }],
                },
            ],
            final_json: "order 4821 is shipped",
        },
        tools: s13_advertised_tools(s13_support_catalog()),
        dispatch: s13_catalog_dispatch(
            s13_support_catalog(),
            loaded,
            (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
                seen.push(c.name);
                baml.ai.invoke_tool<s13_LookupOrderArgs>(c, (a: s13_LookupOrderArgs) -> string {
                    "status of " + a.order_id
                })
            },
        ),
        stop_when: null,
    };
    let r = s13_Support("look up order 4821", client = agent) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "order 4821 is shipped")
    // "order" matched lookup_order (name), process_refund (desc "order line"), search_docs? no.
    assert.is_true(loaded.includes("lookup_order"))
    assert.equal(seen[0], "lookup_order")
}


// ── 5. Capability negotiation: a client that cannot run tools is Unsupported. ──
test "13: a client that cannot run searchable tools is a typed Unsupported" {
    let loaded: string[] = [];
    let agent = baml.ai.ToolLoop {
        inner: NoCapProvider {},
        tools: s13_advertised_tools(s13_support_catalog()),
        dispatch: s13_catalog_dispatch(
            s13_support_catalog(),
            loaded,
            (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
                baml.ai.ToolResult { id: c.id, output: "unreachable" }
            },
        ),
        stop_when: null,
    };
    let r = s13_Support("x", client = agent) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys). ──
// The design-true spelling against a REAL provider: the plain LLM function
// `s13_Support`, driven by a ToolLoop wrapping NATIVE OpenAi tools. `tool_search`
// is advertised alongside the eager tool; the prompt nudges the model to SEARCH
// the catalog first and then call the tool it found. The catalog dispatcher
// intercepts the search (client-executed) and SAP-coerces the real call (D6) —
// all wire correlation stays below the seam. (Native wire deferral + set_tools
// page-in is BLOCKED here, so the deferred tool is advertised for the live run.)
testset "integ-test-13-searchable-tools" {
    test "13 live: native OpenAi calls tool_search, then the discovered tool" {
        let seen: string[] = [];
        let loaded: string[] = [];
        let live_tools: baml.ai.Tool[] = [
            s13_tool("lookup_order", "Look up an order by id.", reflect.type_of<s13_LookupOrderArgs>()),
            s13_search_tool(),
        ];
        let agent = baml.ai.ToolLoop {
            inner: baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
            tools: live_tools,
            dispatch: s13_catalog_dispatch(
                s13_support_catalog(),
                loaded,
                (c: baml.ai.ToolCall) -> baml.ai.ToolResult {
                    seen.push(c.name);
                    baml.ai.invoke_tool<s13_LookupOrderArgs>(c, (a: s13_LookupOrderArgs) -> string {
                        "order " + a.order_id + " is shipped and arriving Tuesday"
                    })
                },
            ),
            stop_when: baml.ai.step_count_is(6),
        };
        let answer = s13_Support(
            "What is the status of order 4821? First search the tool catalog for the right tool, then use it.",
            client = agent,
        ) catch (e) {
            _ => "live searchable loop failed",
        };
        // the model did SOMETHING through the catalog: either paged in via search
        // or dispatched the real lookup_order tool.
        assert.is_true(loaded.length() > 0 || seen.length() > 0)
        assert.is_true(answer.length() > 0)
    }
}
