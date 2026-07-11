// 12 — handoff tools end the loop; they are not dispatched in this process.

class S12TransferArgs {
  reason: string,
}

function s12_transfer_tool() -> root.ai.Tool throws baml.errors.Unsupported {
  let tool = root.ai.Tool.typed<S12TransferArgs>(
    "transfer_to_human",
    "Transfer when a human must take over.",
  );
  root.ai.Tool {
    ...tool,
    handoff: true,
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s12_Investigate(question: string) -> string {
//   client: BepV2ScriptedTools {
//     call_name: "transfer_to_human",
//     call_args: `{"reason":"approval required"}`,
//     final_json: `"unused"`,
//   }
//   prompt: `Investigate ${question}.`
// }

// Manual expansion reference for s12_Investigate.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s12_Investigate_request(...) models s12_Investigate.request(...) with a
//   call-time handoff-tool roster and the no-op dispatcher run_agent requires.
// - s12_Investigate_agent_manual(...) models s12_Investigate.agent(...):
//   root.ai.run_agent(request), returning Handoff before dispatching that call.
// This scenario intentionally has no plain-call s12_Investigate_manual(...) wrapper.
function s12_Investigate_request(
  question: string,
  provider: root.ai.Provider = BepV2ScriptedTools {
    call_name: "transfer_to_human",
    call_args: `{"reason":"approval required"}`,
    final_json: `"unused"`,
  },
  tools: root.ai.Tool[] = [],
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s12_Investigate",
    "ns_ai_scenarios",
    { "question": question },
    tools,
    root.ai.RequestOptions {
      dispatch: (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] { [] },
    },
    {},
    bepv2_prompt_template(`Investigate ${question}.`, false),
  )
}

function s12_Investigate_agent_manual(
  question: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s12_Investigate_request(
    question,
    provider = provider,
    tools = tools,
  ))
}

test "12: a handoff tool yields a typed Handoff without dispatch" {
  let outcome = s12_Investigate_agent_manual(
    "a sensitive refund",
    BepV2ScriptedTools {
      call_name: "transfer_to_human",
      call_args: `{"reason":"approval required"}`,
      final_json: `"unused"`,
    },
    [s12_transfer_tool()],
  );
  match (outcome) {
    let handoff: root.ai.Handoff => assert.equal(handoff.to, "transfer_to_human"),
    _ => assert.is_true(false),
  }
}
