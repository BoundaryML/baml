// 10 — agent budgets are control-flow outcomes, not model output types.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s10_ResearchIt(question: string) -> string {
//   client: BepV2ScriptedTools {
//     call_name: "search",
//     call_args: `{"query":"BEPv2"}`,
//     final_json: `"BEPv2 uses typed requests."`,
//   }
//   prompt: `Research ${question}.`
// }

// This is the desugared code for s10_ResearchIt -----------------------------
function s10_ResearchIt_request(
  question: string,
  provider: root.ai.Provider = BepV2ScriptedTools {
    call_name: "search",
    call_args: `{"query":"BEPv2"}`,
    final_json: `"BEPv2 uses typed requests."`,
  },
  tools: root.ai.Tool[] = [],
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s10_ResearchIt",
    "ns_ai_scenarios",
    { "question": question },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    bepv2_prompt_template(`Research ${question}.`, false),
  )
}

function s10_ResearchIt_agent_manual(
  question: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
  budget: root.ai.Budget? = null,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(
    s10_ResearchIt_request(
      question,
      provider = provider,
      tools = tools,
      dispatch = dispatch,
    ),
    budget = budget,
  )
}

test "10: a completed loop returns Done<T>" {
  let result = s10_ResearchIt_agent_manual(
    "the request design",
    BepV2ScriptedTools {
      call_name: "search",
      call_args: `{"query":"BEPv2"}`,
      final_json: `"BEPv2 uses typed requests."`,
    },
    [s10_search_tool()],
    s10_dispatch,
  );
  match (result) {
    let done: root.ai.Done<string> => assert.is_true(done.value.includes("typed requests")),
    _ => assert.is_true(false),
  }
}

test "10: a deliberate step limit returns BudgetReached" {
  let result = s10_ResearchIt_agent_manual(
    "the request design",
    BepV2ScriptedTools {
      call_name: "search",
      call_args: `{"query":"BEPv2"}`,
      final_json: `"unused"`,
    },
    [s10_search_tool()],
    s10_dispatch,
    budget = root.ai.Budget { max_steps: 1, stop_when: null },
  );
  match (result) {
    let stopped: root.ai.BudgetReached => assert.equal(stopped.steps_taken, 1),
    _ => assert.is_true(false),
  }
}
