// scenario 14 — multi-agent: ../../../../../llm-provider/ideas/scenarios/14-multi-agent/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// Multi-agent sits on a model-decides <-> code-decides spectrum. This file
// exercises the two handoff stories the design covers, both on the D5-honest
// surface (machinery in implementation.baml):
//   (A) DELEGATION      — the router keeps control: a specialist Provider is
//                         called INSIDE the dispatcher; its verdict returns as a
//                         tool RESULT (`s14_handoff_dispatch`).
//   (B) TRUE HANDOFF    — the agent ENDS with a `HandoffRequest`; the caller
//                         routes. Three D5 primitives (run_to_budget arm, the
//                         plain-path `HandoffUnresolved` throw, the interface
//                         `run_tools` arm) plus a caller-side ROUTER that looks
//                         the target up in a roster, forwards the args, and
//                         returns the specialist's answer — with a depth cap and
//                         unknown-target fault.
//
// Offline tier runs against pure-BAML fixtures (no network/keys). Live tier is
// the `testset "integ-test-14-*"` block at the bottom.
//
// Fixtures reused read-only: ScriptedTools / echo_dispatch (scenario 10
// implementation.baml); EchoProvider / NoCapProvider (common/fakes.baml);
// ResearchIt (scenario 10 usage.baml). Router machinery + s14_ fixtures live in
// this dir's implementation.baml.

// ============================================================================
// (A) DELEGATION — the router keeps control (sub-agent-as-tool, honest form)
// ============================================================================

test "14: the dispatcher hands off to a specialist provider" {
    let specialist: baml.ai.Provider = EchoProvider { reply: "specialist verdict" };
    let results = s14_handoff_dispatch(specialist, [baml.ai.ToolCall { id: "h1", name: "escalate", args: "case details" }]);
    assert.equal(results.length(), 1)
    assert.equal(results[0].output, "specialist verdict")
}

test "14: a capability-less specialist degrades to a typed message, not a crash" {
    let results = s14_handoff_dispatch(NoCapProvider {}, [baml.ai.ToolCall { id: "h1", name: "escalate", args: "x" }]);
    assert.equal(results[0].output, "specialist is not callable")
}

// ============================================================================
// (B) TRUE HANDOFF — the D5 honest arm
// ============================================================================
//
// A tool marked `handoff: true` is a transfer, not a dispatch: the loop ENDS and
// the CALLER routes. (The delegation pattern above remains the right shape when
// the original agent should keep owning the conversation.)

test "14: a handoff-marked tool call returns the HandoffRequest arm (run_to_budget)" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 3, final_json: "never reached" },
        tools: s14_transfer_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("route me") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let h: baml.ai.HandoffRequest => "handoff->" + h.to + "@" + h.steps_taken.to_string(),
        let b: baml.ai.Budget<string> => "unexpected budget",
        let v: string => v,
    };
    assert.equal(r, "handoff->lookup@1")
}

test "14: the plain LLM-function path surfaces an unresolved handoff as a TYPED throw" {
    // Design-true spelling: a plain LLM-function call with the loop implied by
    // the ToolLoop CLIENT (DCP §1.6) — the desugared path must produce T, so
    // an unresolved handoff is the typed HandoffUnresolved error here.
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 3, final_json: "never reached" },
        tools: s14_transfer_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = ResearchIt("route me", client = agent) catch (e) {
        let h: baml.errors.HandoffUnresolved => "unresolved->" + h.to,
        _ => "wrong error",
    };
    assert.equal(r, "unresolved->" + "lookup")
}

test "14: run_tools surfaces the HandoffRequest arm on the interface loop too" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 3, final_json: "never reached" };
    let r = match (p) {
        let tp: baml.ai.Tools => {
            let outcome = tp.run_tools<string>("route me", s14_transfer_tools(), echo_dispatch) catch (e) {
                _ => "loop failed"
            };
            match (outcome) {
                let h: baml.ai.HandoffRequest => "handoff->" + h.to,
                _ => "no handoff",
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "handoff->lookup")
}

// ============================================================================
// (B') TRUE HANDOFF — the caller-side ROUTER (the design's transfer_to_* intent)
// ============================================================================
//
// The design wanted `Handoff` to swap the active provider inside a Tools loop.
// D5 rejected that; the honest shape is a caller-side router that catches the
// `HandoffRequest` arm, looks the target up in a roster (name -> Provider),
// forwards the args payload, and returns the specialist's answer. The depth-cap
// and unknown-target footguns become typed faults on the caller side.

test "14: the router forwards a handoff to the roster target and returns its answer" {
    // A Triage agent whose only move is `transfer_to_billing`. The roster routes
    // that to a Billing specialist that answers immediately (turns_wanted: 0).
    let triage = s14_Specialist {
        provider: s14_ScriptedRouter { transfer_to: "transfer_to_billing" },
        tools: s14_router_tools(),
    };
    let router = s14_Router {
        agents: {
            "transfer_to_billing": s14_Specialist {
                provider: ScriptedTools { turns_wanted: 0, final_json: "billing resolved your refund" },
                tools: [],
            },
        },
        dispatch: echo_dispatch,
        max_depth: 4,
    };
    let answer = router.route<string>(triage, "I was charged twice.") catch (e) {
        _ => "router failed",
    };
    assert.equal(answer, "billing resolved your refund")
}

test "14: an unknown handoff target is a typed s14_NoSuchHandoffTarget on the caller side" {
    let triage = s14_Specialist {
        provider: s14_ScriptedRouter { transfer_to: "transfer_to_billing" },
        tools: s14_router_tools(),
    };
    // Roster has only a tech target; the transfer to billing has nowhere to go.
    let router = s14_Router {
        agents: {
            "transfer_to_tech": s14_Specialist {
                provider: ScriptedTools { turns_wanted: 0, final_json: "tech says restart" },
                tools: [],
            },
        },
        dispatch: echo_dispatch,
        max_depth: 4,
    };
    let r = router.route<string>(triage, "help") catch (e) {
        let n: s14_NoSuchHandoffTarget => "no-target->" + n.to_name,
        _ => "wrong error",
    };
    assert.equal(r, "no-target->transfer_to_billing")
}

test "14: handoff ping-pong is capped by a typed s14_HandoffDepthExceeded" {
    let triage = s14_Specialist {
        provider: s14_ScriptedRouter { transfer_to: "transfer_to_billing" },
        tools: s14_router_tools(),
    };
    // The billing target ALSO only ever transfers back to billing -> the chain
    // would ping-pong forever without the caller's depth cap.
    let router = s14_Router {
        agents: {
            "transfer_to_billing": s14_Specialist {
                provider: s14_ScriptedRouter { transfer_to: "transfer_to_billing" },
                tools: s14_router_tools(),
            },
        },
        dispatch: echo_dispatch,
        max_depth: 2,
    };
    let r = router.route<string>(triage, "loop me") catch (e) {
        let d: s14_HandoffDepthExceeded => "capped@" + d.max_depth.to_string() + "->" + d.last_target,
        _ => "wrong error",
    };
    assert.equal(r, "capped@2->transfer_to_billing")
}

// ============================================================================
// Live tier — excluded by default; run via `-i "integ-test*::"` with keys.
// ============================================================================
//
// The design-true handoff against a REAL provider: a Triage agent (native OpenAi
// tools) is offered ONE handoff-marked transfer tool. When the model calls it the
// loop ENDS with a HandoffRequest; the caller-side router forwards to a Billing
// specialist that answers. Proves the whole handoff arc — model-decided transfer,
// caller-routed forward — with the wire tool-calling below the seam.

testset "integ-test-14-multi-agent" {
    test "14 live: the router forwards a real model's transfer to a specialist" {
        let triage_provider: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let triage = s14_Specialist {
            provider: triage_provider,
            tools: [baml.ai.Tool {
                name: "transfer_to_billing",
                description: "Transfer the conversation to the billing specialist. Call this for any billing, invoice, refund, or charge question. Do not answer directly.",
                parameters: "{}",
                handoff: true,
            }],
        };
        let router = s14_Router {
            agents: {
                "transfer_to_billing": s14_Specialist {
                    provider: ScriptedTools { turns_wanted: 0, final_json: "Your duplicate charge has been refunded." },
                    tools: [],
                },
            },
            dispatch: echo_dispatch,
            max_depth: 4,
        };
        let answer = router.route<string>(triage, "I was charged twice for my subscription this month.") catch (e) {
            _ => "router failed",
        };
        assert.is_true(answer.to_lower_case().includes("refund"))
    }
}
