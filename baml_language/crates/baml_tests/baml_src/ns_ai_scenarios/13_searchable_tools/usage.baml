// 13 — an MCP catalog can grow the concrete provider tool roster between turns.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s13_Support(question: string) -> string {
//   client: root.ai.FakeToolProvider {
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
  provider: root.ai.Provider = root.ai.FakeToolProvider {
    call_name: "tool_search",
    call_args: `{"query":"order status"}`,
    final_json: `"Order 4821 arrives Tuesday."`,
  },
  registry: root.ai.ToolRegistry = root.ai.ToolRegistry.new([s13_search_tool()]),
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s13_Support",
    "ns_ai_scenarios",
    { "question": question },
    registry.snapshot(),
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    root.ai.prompt_template(
      `Help with this support request: ${question}
First call tool_search. Then call the newly discovered MCP order tool before answering.`,
      false,
    ),
  ).with_tool_registry(registry)
}

function s13_Support_agent_manual(
  question: string,
  provider: root.ai.Provider,
  registry: root.ai.ToolRegistry,
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s13_Support_request(
    question,
    provider = provider,
    registry = registry,
    dispatch = dispatch,
  ))
}

test "13: tool search returns a small catalog page" {
  let events: string[] = [];
  let registry = root.ai.ToolRegistry.new([s13_search_tool()]);
  let result = s13_dispatch(events, registry)([
    root.ai.ToolCall { id: "search-1", name: "tool_search", args: `{"query":"order"}` },
  ]);
  assert.equal(events, ["loaded:mcp__orders__lookup_order"])
  assert.equal(registry.snapshot().length(), 2)
  let output = result.at(0)?.output ?? "";
  assert.is_true(output.includes("mcp__orders__lookup_order"))
}

test "13: OpenAI receives the discovered MCP schema only after registry sync" {
  let events: string[] = [];
  let registry = root.ai.ToolRegistry.new([s13_search_tool()]);
  let provider = root.ai.OpenAi { model: "test", api_key: "unused", base_url: null };
  let request = s13_Support_request(
    "Where is order 4821?",
    provider = provider,
    registry = registry,
    dispatch = s13_dispatch(events, registry),
  );
  let transcript = provider.begin<string>(request);
  assert.equal(transcript.tools.length(), 1)
  assert.equal(baml.json.path_or(transcript.tools[0], ".function.name", ""), "tool_search")

  let _ = s13_dispatch(events, registry)([
    root.ai.ToolCall { id: "search-1", name: "tool_search", args: `{"query":"order"}` },
  ]);
  let updated = provider.sync_tools(transcript, registry.snapshot());
  assert.equal(updated.tools.length(), 2)
  assert.equal(
    baml.json.path_or(updated.tools[1], ".function.name", ""),
    "mcp__orders__lookup_order",
  )
}

test "13: the searchable roster drives the manual agent request" {
  let events: string[] = [];
  let registry = root.ai.ToolRegistry.new([s13_search_tool()]);
  let outcome = s13_Support_agent_manual(
    "Where is order 4821?",
    root.ai.FakeToolProvider {
      call_name: "tool_search",
      call_args: `{"query":"order status"}`,
      final_json: `"Order 4821 arrives Tuesday."`,
    },
    registry,
    s13_dispatch(events, registry),
  );
  assert.equal(events, ["loaded:mcp__orders__lookup_order"])
  match (outcome) {
    let done: root.ai.Done<string> => assert.is_true(done.value.includes("Tuesday")),
    _ => assert.is_true(false),
  }
}

testset "integ-test-13-searchable-tools" {
  test "13 live: OpenAI calls tool_search and continues with catalog context" {
    let events: string[] = [];
    let registry = root.ai.ToolRegistry.new([s13_search_tool()]);
    let outcome = s13_Support_agent_manual(
      "What is the status of order 4821? Search the catalog first.",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
      registry,
      s13_dispatch(events, registry),
    );
    assert.is_true(events.includes("loaded:mcp__orders__lookup_order"))
    assert.is_true(events.includes("called:mcp__orders__lookup_order"))
    match (outcome) {
      let done: root.ai.Done<string> => assert.is_true(done.value.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
