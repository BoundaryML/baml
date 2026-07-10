// 12 — handoff tools end the loop; they are not dispatched in this process.

class S12TransferArgs {
  reason: string,
}

function s12_transfer_tool() -> root.ai.Tool throws baml.errors.Unsupported {
  let tool = root.ai.Tool.from_type(
    "transfer_to_human",
    "Transfer when a human must take over.",
    reflect.type_of<S12TransferArgs>(),
  );
  root.ai.Tool {
    name: tool.name,
    description: tool.description,
    parameters: tool.parameters,
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

// This is the desugared code for s12_Investigate ----------------------------
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
