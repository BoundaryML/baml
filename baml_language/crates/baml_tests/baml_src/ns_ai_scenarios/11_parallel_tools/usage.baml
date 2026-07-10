// 11 — dispatch policy decides whether requested calls execute concurrently.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s11_Research(question: string) -> string {
//   client: BepV2ScriptedTools {
//     call_name: "search",
//     call_args: `{"query":"Tokyo"}`,
//     final_json: `"Tokyo is mild."`,
//   }
//   prompt: `Research ${question}.`
// }

// This is the desugared code for s11_Research -------------------------------
function s11_Research_request(
  question: string,
  provider: root.ai.Provider = BepV2ScriptedTools {
    call_name: "search",
    call_args: `{"query":"Tokyo"}`,
    final_json: `"Tokyo is mild."`,
  },
  tools: root.ai.Tool[] = [],
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s11_Research",
    "ns_ai_scenarios",
    { "question": question },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    bepv2_prompt_template(`Research ${question}.`, false),
  )
}

function s11_Research_agent_manual(
  question: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s11_Research_request(
    question,
    provider = provider,
    tools = tools,
    dispatch = dispatch,
  ))
}

test "11: a parallel dispatcher returns results in call order" {
  let audit: string[] = [];
  let dispatch = s11_parallel_dispatch(audit);
  let results = dispatch([
    root.ai.ToolCall { id: "a", name: "search", args: `{"query":"Tokyo"}` },
    root.ai.ToolCall { id: "b", name: "search", args: `{"query":"Paris"}` },
  ]);
  assert.equal(results.map((r: root.ai.ToolResult) -> string { r.id }), ["a", "b"])
  assert.equal(audit.length(), 2)
}

testset "integ-test-11-parallel-tools" {
  test "11 live: OpenAI tool calls use the parallel call-time dispatcher" {
    let audit: string[] = [];
    let outcome = s11_Research_agent_manual(
      "Compare Tokyo and Paris. Call search for both.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
      [s11_search_tool()],
      s11_parallel_dispatch(audit),
    );
    assert.is_true(audit.length() > 0)
    match (outcome) {
      let done: root.ai.Done<string> => assert.is_true(done.value.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
