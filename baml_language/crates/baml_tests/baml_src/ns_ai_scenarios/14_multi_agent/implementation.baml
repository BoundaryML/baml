// scenario 14 — multi-agent: ../../../../../llm-provider/ideas/scenarios/14-multi-agent/implement.baml
//   implementation.baml — LIBRARY/std-lib-author code.
//
// The design's implement.baml made `Handoff` a Tools-COMBINATOR that synthesized
// `transfer_to_*` tools and SWAPPED the active provider internally (submit
// re-`begin`s a different provider). The master plan's D5 decision REJECTED that
// ("handoff through the wrong door") in favor of an HONEST return arm, which now
// ships in `baml.ai`:
//   - `Tool { handoff: true }` marks a transfer tool;
//   - `run_tools`/`$run_tools`/`ToolLoop.run_to_budget` return the honest sum
//     `T | Budget<T> | HandoffRequest { to, args, steps_taken }` — the loop ENDS
//     and the CALLER routes;
//   - the plain LLM-function path (no room for the extra arms in `-> T`) throws a
//     typed `baml.errors.HandoffUnresolved`.
//
// So this scenario tells BOTH stories the design covers:
//   (A) DELEGATION — the ROUTER keeps control: a specialist Provider is called
//       INSIDE the dispatcher (`s14_handoff_dispatch`). The conversation never
//       leaves the original agent; the specialist's verdict comes back as a tool
//       RESULT. This is the design's sub-agent-as-tool shape, honestly spelled.
//   (B) TRUE HANDOFF — the design's `transfer_to_*` intent, D5-honest: the agent
//       ENDS with a `HandoffRequest`, and a caller-side ROUTER (`s14_Router`)
//       looks the target up in a roster (name -> Provider), forwards the args
//       payload, and returns the specialist's answer. The design's depth-cap
//       (`s14_HandoffDepthExceeded`) and unknown-target (`s14_NoSuchHandoffTarget`)
//       errors live HERE, on the caller side — the honest home the D5 arm gives
//       them, instead of buried inside a provider-swapping `submit`.
//
// Everything net-new here is `s14_`-prefixed. Shared fixtures reused read-only:
//   ScriptedTools / echo_dispatch          (scenario 10 implementation.baml)
//   EchoProvider / NoCapProvider           (common/fakes.baml)
//   ResearchIt / DeadDeclared              (scenario 10 usage.baml)

// ============================================================================
// (A) DELEGATION — the router keeps control (sub-agent-as-tool, honest form)
// ============================================================================
//
// A "handoff" that keeps the conversation with the caller is just calling another
// Provider inside the dispatcher: the specialist runs, and its verdict is fed
// back as an ordinary tool RESULT. Deterministic orchestration = plain
// composition; the specialist is itself a `Provider` matched for `HttpProvider`.

function s14_handoff_dispatch(
    specialist: baml.ai.Provider,
    calls: baml.ai.ToolCall[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = match (specialist) {
            let h: baml.ai.HttpProvider => {
                h.call<string>(c.args) catch (e) {
                    _ => "sub-agent failed"
                }
            },
            _ => "specialist is not callable",
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}

// ============================================================================
// (B) TRUE HANDOFF — the D5 honest arm + a caller-side router
// ============================================================================

// The transfer tools the D5 slice tests advertise. A single handoff-marked tool
// named `lookup`: calling it ends the loop with a `HandoffRequest{to:"lookup"}`
// rather than dispatching. (Kept EXACTLY stable — the three D5 tests below rely
// on the name and the `handoff: true` marking.)
function s14_transfer_tools() -> baml.ai.Tool[] {
    [baml.ai.Tool {
        name: "lookup",
        description: "Transfer this conversation to the lookup specialist.",
        parameters: "{}",
        handoff: true,
    }]
}

// --- the design-shaped router surface ----------------------------------------
//
// A caller-side router: it runs an agent, catches the `HandoffRequest` arm, looks
// the target up in a roster (name -> specialist), forwards the raw args payload
// as the child prompt, and repeats until an agent produces a value. The two
// footgun errors the background names (uncapped ping-pong, bad target) become
// typed faults raised HERE, on the caller side.

// A named specialist: the Provider to run + the tools it is offered. Those tools
// MAY themselves include handoff-marked transfer tools, so a specialist can hand
// off ONWARD — which is what the depth cap guards against.
class s14_Specialist {
    provider: baml.ai.Provider,
    tools: baml.ai.Tool[],
}

// --- caller-side handoff faults (implement the ToolError channel) -------------
// Deliberate ORCHESTRATION faults, not transport/parse faults, so every
// classifier reports false. Both `implements baml.errors.ToolError`, so throwing
// them is legal on the router's ToolError channel and a caller catches them by
// their concrete type.

/// The handoff chain exceeded the caller's depth cap (the ping-pong guard the
/// SDKs leave uncapped — `max_turns` caps ONE loop, not handoff depth).
class s14_HandoffDepthExceeded {
    max_depth: int,
    last_target: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { true }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}

/// The model asked to transfer to a name that is not in the roster.
class s14_NoSuchHandoffTarget {
    to_name: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { true }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { true }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}

/// The roster + policy that routes handoffs. `agents` maps a transfer-tool name
/// (`HandoffRequest.to`) to the specialist that should own the conversation next.
class s14_Router {
    agents: map<string, s14_Specialist>,
    dispatch: (baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never,
    max_depth: int,

    /// Run `start` against `prompt`; whenever the running agent ends with a
    /// `HandoffRequest`, look the target up in the roster, forward the args
    /// payload, and continue on that specialist — until an agent yields a value.
    function route<T>(self, start: s14_Specialist, prompt: string) -> T
        throws baml.errors.ToolError | baml.errors.UnknownError {
        let current = start;
        let cur_prompt = prompt;
        let depth = 0;
        while (true) {
            let agent = baml.ai.ToolLoop {
                inner: current.provider,
                tools: current.tools,
                dispatch: self.dispatch,
                stop_when: null,
            };
            let outcome = agent.run_to_budget<T>(cur_prompt);
            match (outcome) {
                let h: baml.ai.HandoffRequest => {
                    depth = depth + 1;
                    if (depth > self.max_depth) {
                        throw s14_HandoffDepthExceeded { max_depth: self.max_depth, last_target: h.to };
                    }
                    let next = self.agents.get(h.to);
                    match (next) {
                        null => throw s14_NoSuchHandoffTarget { to_name: h.to },
                        let s: s14_Specialist => {
                            current = s;
                            cur_prompt = h.args;   // forward the args payload to the child
                        },
                    }
                },
                let b: baml.ai.Budget<T> => {
                    // No stop policy is configured, so this arm is unreachable at
                    // runtime; normalize it if a future policy ever surfaces it.
                    throw baml.errors.UnknownError { data: null, message: ["router: unexpected budget stop"] };
                },
                let v: T => {
                    return v;
                },
            }
        }
        baml.sys.panic("unreachable")
    }
}

// --- offline fixtures for the router tests ------------------------------------

// The transfer tools the router advertises: one handoff-marked tool per target
// name. Calling `transfer_to_billing` ends the agent with
// `HandoffRequest{to:"transfer_to_billing"}`; the router keys the roster on that.
function s14_router_tools() -> baml.ai.Tool[] {
    [
        baml.ai.Tool {
            name: "transfer_to_billing",
            description: "Transfer to the billing specialist.",
            parameters: "{}",
            handoff: true,
        },
        baml.ai.Tool {
            name: "transfer_to_tech",
            description: "Transfer to the tech-support specialist.",
            parameters: "{}",
            handoff: true,
        },
    ]
}

/// An offline Tools provider that always requests one handoff-marked transfer
/// tool named `transfer_to`, so `run_to_budget` yields
/// `HandoffRequest{to: transfer_to}` on the first turn. Models "the router model
/// calls transfer_to_*" with no network.
class s14_ScriptedRouter {
    transfer_to: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = s14_RouterState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s14_RouterState throws never {
            s14_RouterState { turns: 0 }
        }
        function step<T>(self, t: s14_RouterState) -> T | baml.ai.ToolCalls throws never {
            baml.ai.ToolCalls {
                calls: [baml.ai.ToolCall { id: "xfer_1", name: self.transfer_to, args: "{\"reason\":\"needs a specialist\"}" }],
            }
        }
        function submit(self, t: s14_RouterState, results: baml.ai.ToolResult[]) -> s14_RouterState throws never {
            s14_RouterState { turns: t.turns + 1 }
        }
    }
}

class s14_RouterState {
    turns: int,
}
