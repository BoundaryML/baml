// 13 — a catalog is a source of call-time tools, not a new task modifier.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s13_Support(question: string) -> string {
//   client: BepV2ScriptedTools {
//     call_name: "tool_search",
//     call_args: `{"query":"order status"}`,
//     final_json: `"Order 4821 arrives Tuesday."`,
//   }
//   prompt: `Help with this support request: ${question}`
// }

// Manual expansion reference for s13_Support.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s13_Support_request(...) models s13_Support.request(...) with its call-time
//   tool roster and dispatcher.
// - s13_Support_agent_manual(...) models s13_Support.agent(...):
//   root.ai.run_agent(request), preserving the typed outcome union.
// The searchable catalog is ordinary application data, not another task modifier.
// This scenario intentionally has no plain-call s13_Support_manual(...) wrapper.
function s13_Support_request(
  question: string,
  provider: root.ai.Provider = BepV2ScriptedTools {
    call_name: "tool_search",
    call_args: `{"query":"order status"}`,
    final_json: `"Order 4821 arrives Tuesday."`,
  },
  tools: root.ai.Tool[] = [],
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s13_Support",
    "ns_ai_scenarios",
    { "question": question },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    bepv2_prompt_template(`Help with this support request: ${question}`, false),
  )
}

function s13_Support_agent_manual(
  question: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s13_Support_request(
    question,
    provider = provider,
    tools = tools,
    dispatch = dispatch,
  ))
}

test "13: tool search returns a small catalog page" {
  let loaded: string[] = [];
  let result = s13_dispatch(loaded)([
    root.ai.ToolCall { id: "search-1", name: "tool_search", args: `{"query":"order"}` },
  ]);
  assert.equal(loaded, ["lookup_order"])
  let output = result.at(0)?.output ?? "";
  assert.is_true(output.includes("lookup_order"))
}

test "13: the searchable roster drives the manual agent request" {
  let loaded: string[] = [];
  let outcome = s13_Support_agent_manual(
    "Where is order 4821?",
    BepV2ScriptedTools {
      call_name: "tool_search",
      call_args: `{"query":"order status"}`,
      final_json: `"Order 4821 arrives Tuesday."`,
    },
    [s13_search_tool(), s13_lookup_tool()],
    s13_dispatch(loaded),
  );
  assert.equal(loaded, ["lookup_order"])
  match (outcome) {
    let done: root.ai.Done<string> => assert.is_true(done.value.includes("Tuesday")),
    _ => assert.is_true(false),
  }
}

testset "integ-test-13-searchable-tools" {
  test "13 live: OpenAI calls tool_search and continues with catalog context" {
    let loaded: string[] = [];
    let outcome = s13_Support_agent_manual(
      "What is the status of order 4821? Search the catalog first.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
      [s13_search_tool(), s13_lookup_tool()],
      s13_dispatch(loaded),
    );
    assert.is_true(loaded.length() > 0)
    match (outcome) {
      let done: root.ai.Done<string> => assert.is_true(done.value.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
