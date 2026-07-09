// scenario 12 — tool taxonomy: ../../../../../llm-provider/ideas/scenarios/12-tool-taxonomy/README.md
//
// The design's teaching point: "a Tool" is several different things that differ
// by WHO executes the call and WHERE — so *execution location* must be a
// first-class marker, not just name+schema. The design lifted that into a
// `ToolKind` union (FunctionTool / HostedTool / ComputerUseTool / McpServerTool)
// plus HITL-as-a-wrapper.
//
// On the REAL surface the taxonomy collapses onto three routes the stdlib Tools
// loop actually distinguishes:
//   - CLIENT-EXECUTED — a plain `baml.ai.Tool` the loop DISPATCHES via your
//       dispatcher; a `ToolResult` flows back (design's FunctionTool + the
//       client-side leg of ComputerUseTool).
//   - HANDOFF-SHAPED  — a `baml.ai.Tool { handoff: true }`: the loop does NOT
//       dispatch it; it ENDS and returns a `HandoffRequest { to, args,
//       steps_taken }` (run_tools / run_to_budget) — or throws
//       `baml.errors.HandoffUnresolved` on the plain LLM-function path. This is
//       the new first-class kind the design only had as a wrapper.
//   - PROVIDER-EXECUTED — hosted tools (web_search / code_execution), remote MCP,
//       and provider-driven computer-use: the provider runs it and you can only
//       OBSERVE. There is NO host seam for this today (no `ToolActivity`/observe
//       channel on `Tools`, and `baml.ai.Tool` carries no hosted marker) — the
//       shape/negotiation story is BLOCKED P8. Kept in the taxonomy, routed to a
//       typed "unsupported here" outcome.
//
// Same loop, different routing keyed off the marker — that IS the taxonomy.
// Reuses scenario 10's ScriptedTools/echo_dispatch and common/fakes read-only.


// ── The taxonomy: execution location as a first-class marker ─────────────────

enum ToolKind {
    ClientExecuted,    // your code runs it; a ToolResult flows back
    ProviderExecuted,  // provider runs it; you observe only (no host seam — BLOCKED P8)
    Handoff,           // transfer: the loop ends, the caller routes
}

/// Where each kind routes in the real Tools loop. This is the dispatch-side
/// consequence of the marker — the whole point of lifting execution location
/// into the type.
function s12_route_of(kind: ToolKind) -> string throws never {
    match (kind) {
        ToolKind.ClientExecuted => "loop dispatches it; ToolResult flows back",
        ToolKind.ProviderExecuted => "provider executes; caller observes only (no host seam — BLOCKED P8)",
        ToolKind.Handoff => "loop ends; caller routes the HandoffRequest",
    }
}

/// Classify a REAL `baml.ai.Tool` by its markers. A `baml.ai.Tool` can only be
/// client-executed or a handoff (`handoff: true`) — provider-executed tools have
/// no representation on this surface at all, which is precisely the P8 gap.
function s12_kind_of(tool: baml.ai.Tool) -> ToolKind throws never {
    if (tool.handoff ?? false) {
        ToolKind.Handoff
    } else {
        ToolKind.ClientExecuted
    }
}

test "12: the execution-location marker drives dispatch-side routing" {
    assert.equal(s12_route_of(ToolKind.ClientExecuted), "loop dispatches it; ToolResult flows back")
    assert.equal(s12_route_of(ToolKind.Handoff), "loop ends; caller routes the HandoffRequest")
    // provider-executed stays in the taxonomy but routes to the blocked outcome.
    assert.is_true(s12_route_of(ToolKind.ProviderExecuted).includes("BLOCKED P8"))
}

test "12: a real Tool classifies by its markers (client-executed vs handoff)" {
    let client_tool = baml.ai.Tool { name: "search", description: "Search the KB.", parameters: "{}" };
    let handoff_tool = baml.ai.Tool {
        name: "transfer_to_billing",
        description: "Transfer this conversation to billing.",
        parameters: "{}",
        handoff: true,
    };
    assert.equal(s12_kind_of(client_tool), ToolKind.ClientExecuted)
    assert.equal(s12_kind_of(handoff_tool), ToolKind.Handoff)
}


// ── A scripted provider that calls a SEQUENCE of named tools ─────────────────
// scenario 10's ScriptedTools only ever calls "lookup"; to show two kinds
// routing differently in ONE loop we need to script distinct tool names per turn.

class S12ScriptState {
    turn: int,
    script: string[],   // tool name to request on each turn, in order
    final_json: string,
}

class S12ScriptedAgent {
    script: string[],
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = S12ScriptState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> S12ScriptState throws never {
            S12ScriptState { turn: 0, script: self.script, final_json: self.final_json }
        }
        function step<T>(self, t: S12ScriptState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.turn < t.script.length()) {
                return baml.ai.ToolCalls {
                    calls: [baml.ai.ToolCall {
                        id: "c" + t.turn.to_string(),
                        name: t.script[t.turn],
                        args: "{}",
                    }],
                };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s12 final parse failed"] },
            }
        }
        function submit(self, t: S12ScriptState, results: baml.ai.ToolResult[]) -> S12ScriptState throws never {
            S12ScriptState { turn: t.turn + 1, script: t.script, final_json: t.final_json }
        }
    }
}

/// A heterogeneous tool set: one client-executed tool and one handoff tool — the
/// two routes the real loop distinguishes, side by side.
function s12_mixed_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool { name: "search", description: "Search the knowledge base.", parameters: "{}" },
        baml.ai.Tool {
            name: "transfer_to_billing",
            description: "Transfer this conversation to the billing specialist.",
            parameters: "{}",
            handoff: true,
        },
    ]
}


// ── Behavioral routing: same loop, different route per marker ─────────────────

test "12: a client-executed tool routes through dispatch to a final value" {
    let agent = baml.ai.ToolLoop {
        inner: S12ScriptedAgent { script: ["search"], final_json: "resolved" },
        tools: s12_mixed_tools(),
        dispatch: echo_dispatch,   // client-executed: YOUR code runs, ToolResult flows back
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("look it up") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let v: string => v,
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let b: baml.ai.Budget<string> => "unexpected budget",
    };
    assert.equal(r, "resolved")
}

test "12: a handoff-shaped tool ends the loop with a HandoffRequest" {
    let agent = baml.ai.ToolLoop {
        inner: S12ScriptedAgent { script: ["transfer_to_billing"], final_json: "never reached" },
        tools: s12_mixed_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("route me") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let h: baml.ai.HandoffRequest => "handoff->" + h.to + "@" + h.steps_taken.to_string(),
        let v: string => v,
        let b: baml.ai.Budget<string> => "unexpected budget",
    };
    assert.equal(r, "handoff->transfer_to_billing@1")
}

test "12: mixed kinds in one run — dispatch the client tool, THEN end on the handoff" {
    // Turn 0 calls "search" (client-executed → dispatched); turn 1 calls
    // "transfer_to_billing" (handoff → loop ends). One loop, two routes, decided
    // entirely by each tool's marker.
    let agent = baml.ai.ToolLoop {
        inner: S12ScriptedAgent {
            script: ["search", "transfer_to_billing"],
            final_json: "never reached",
        },
        tools: s12_mixed_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("investigate then route") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let h: baml.ai.HandoffRequest => "handoff->" + h.to + "@" + h.steps_taken.to_string(),
        let v: string => v,
        let b: baml.ai.Budget<string> => "unexpected budget",
    };
    // steps_taken == 2: the client tool turn ran and was dispatched before the
    // handoff turn ended the loop.
    assert.equal(r, "handoff->transfer_to_billing@2")
}


// ── The design-true spelling: a plain LLM function, loop implied by the client ─
// PREFER LLM FUNCTIONS (DCP §1.6): the app author writes an ordinary one-shot
// function; a `ToolLoop` client runs begin/step/submit and the desugared path
// must produce T — so a client-executed run returns the value, and an
// unresolved handoff surfaces as the typed `HandoffUnresolved`.

client<llm> s12_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function S12Investigate(q: string) -> string {
    client s12_Dead
    prompt #"Investigate with your tools: {{ q }}"#
}

test "12: the LLM-function path returns T when every tool is client-executed" {
    let agent = baml.ai.ToolLoop {
        inner: S12ScriptedAgent { script: ["search"], final_json: "case closed" },
        tools: s12_mixed_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = S12Investigate("account 42", client = agent) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "case closed")
}

test "12: the LLM-function path surfaces an unresolved handoff as a TYPED throw" {
    // The plain call must produce T, so a handoff (no T arm) is the typed error.
    let agent = baml.ai.ToolLoop {
        inner: S12ScriptedAgent { script: ["transfer_to_billing"], final_json: "never reached" },
        tools: s12_mixed_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = S12Investigate("route me", client = agent) catch (e) {
        let h: baml.errors.HandoffUnresolved => "unresolved->" + h.to,
        _ => "wrong error",
    };
    assert.equal(r, "unresolved->transfer_to_billing")
}


// ── PROVIDER-EXECUTED — the blocked leg (P8) ─────────────────────────────────
// Hosted tools (web_search / code_execution), remote MCP, and provider-driven
// computer-use all share one shape: the provider runs the call and you can only
// OBSERVE it. The real `Tools` seam has no observe channel (no `ToolActivity`,
// no `step_mixed`/`run_mixed`), and `baml.ai.Tool` carries no hosted marker — so
// there is nothing to declare and nothing to drive. It stays in the taxonomy and
// routes to a typed "unsupported here" outcome until the host surface lands.

test "12: provider-executed tools are the negotiation/shape gap (BLOCKED P8)" {
    // No `baml.ai.Tool` can BE provider-executed, so classification never yields
    // it from a real tool; the route description records the gap explicitly.
    let route = s12_route_of(ToolKind.ProviderExecuted);
    assert.is_true(route.includes("provider executes"))
    assert.is_true(route.includes("observes only"))
    assert.is_true(route.includes("BLOCKED P8"))
}
