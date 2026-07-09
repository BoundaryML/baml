// scenario 10 — agentic loop + stop: ../../../../../llm-provider/ideas/scenarios/10-agentic-loop/implement.baml
//   implementation.baml — LIBRARY/std-lib-author code.
//
// The design's implement.baml hand-rolled two whole providers (OpenAI id-keyed,
// Gemini name+position) each `implements Tools`, restated the `Tools` interface
// with a default `run_tools`, and introduced a `Bounded` combinator that
// OVERRIDES `run_tools` with a stop-predicate loop returning `Budget<T>`, plus
// net-new surface (`History`/`StepInfo`, `step_count_is`/`has_tool_call`/`any_of`,
// per-turn tool filtering). On the REAL surface all of that already ships in
// `baml.ai` (REALIZED.md #10):
//   - the `Tools` interface (`begin`/`step`/`submit` + the default `run_tools`),
//   - `baml.ai.ToolLoop { inner, tools, dispatch, stop_when }` IS the design's
//     `Bounded` — the agentic loop AS a provider (`implements HttpProvider`), so
//     a plain LLM-function call with a ToolLoop client drives the loop,
//   - the stop-policy vocabulary `step_count_is` / `has_tool_call` / `any_of`
//     over `LoopInfo` (the design's `StepInfo`),
//   - `ToolLoop.run_to_budget<T> -> T | Budget<T> | HandoffRequest` (the design's
//     "partial result at the cap", now an honest SUM arm, not a thrown control
//     signal), and the plain-path budget hit as a typed `LoopBudgetExceeded`.
// So the only library-author code THIS scenario still owns is an OFFLINE,
// deterministic `Tools` provider to drive the loop with no network — the design's
// two wire providers collapse to one scripted fixture, since the id-vs-name
// correlation lesson lives BELOW the seam in the native providers (scenario 09).
//
// DESIGN DIVERGENCE (D7 gap — noted, not built): the design's `per_turn_tools`
// filter `(int, History) -> Tool[]` needs a `set_tools` seam on `Tools` so the
// provider can re-render its own wire envelope mid-loop. `baml.ai.ToolLoop` fixes
// the tool set at `begin` (see tools.baml's own note). The design-true shape would
// be a fourth `ToolLoop` field:
//     per_turn_tools: ((int) -> baml.ai.Tool[])?   // recompute exposed tools per turn
// consulted before each `submit` and threaded through a `Tools.set_tools(t, tools)`
// method. Left as a comment per the stdlib-freeze constraint.
//
// SHARED FIXTURES — these three names/signatures/semantics are load-bearing for
// OTHER scenario dirs (09 tool-calling, 14 multi-agent, …). Keep them EXACTLY
// stable: `ScriptedTools`, `echo_dispatch`, `lookup_tools`.

/// Scripted transcript: demand `turns_wanted` tool turns, then produce
/// `final_json` as the value (SAP-parsed into the caller's `T`).
class ScriptState {
    turns_wanted: int,
    turns_done: int,
    final_json: string,
}

/// An offline Tools-capable provider (no network): requests one `lookup` tool
/// call per turn until the script is exhausted, then answers. This is the whole
/// scenario's driver — the design's OpenAITools/GeminiTools wire threading lives
/// natively in `baml.ai` (scenario 09), so here the loop is exercised offline.
class ScriptedTools {
    turns_wanted: int,
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = ScriptState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> ScriptState throws never {
            ScriptState {
                turns_wanted: self.turns_wanted,
                turns_done: 0,
                final_json: self.final_json,
            }
        }
        function step<T>(self, t: ScriptState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.turns_done < t.turns_wanted) {
                return baml.ai.ToolCalls {
                    calls: [baml.ai.ToolCall { id: "call_1", name: "lookup", args: "{}" }],
                };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["script final parse failed"] },
            }
        }
        function submit(self, t: ScriptState, results: baml.ai.ToolResult[]) -> ScriptState throws never {
            ScriptState {
                turns_wanted: t.turns_wanted,
                turns_done: t.turns_done + 1,
                final_json: t.final_json,
            }
        }
    }
}

/// A provider with NO Tools capability, for the negotiation-failure test (the
/// loop's `inner` cannot drive a non-Tools provider -> typed `Unsupported`).
class ScriptedNotTools {
    implements baml.ai.Provider {}
}

/// The app's tool dispatcher: echo a fixed result for each requested call.
/// (Shared fixture — keep the name/signature/semantics stable.)
function echo_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        out.push(baml.ai.ToolResult { id: c.id, output: "looked-up" });
    }
    out
}

/// The single-tool registry the scripted loop advertises.
/// (Shared fixture — keep the name/signature/semantics stable.)
function lookup_tools() -> baml.ai.Tool[] {
    [baml.ai.Tool { name: "lookup", description: "Look something up.", parameters: "{}" }]
}
