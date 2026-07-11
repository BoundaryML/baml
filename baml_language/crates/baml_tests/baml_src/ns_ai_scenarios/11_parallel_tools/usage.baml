// 11 — dispatch policy decides whether requested calls execute concurrently.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s11_Research(question: string) -> string {
//   client: root.ai.FixtureTools {
//     call_name: "search",
//     call_args: `{"query":"Tokyo"}`,
//     final_json: `"Tokyo is mild."`,
//   }
//   prompt: `Research ${question}.`
// }

// Manual expansion reference for s11_Research.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s11_Research_request(...) models s11_Research.request(...); its optional
//   tools and dispatcher carry the call-time s11_Research.agent(...) inputs.
// - s11_Research_agent_manual(...) models s11_Research.agent(...):
//   root.ai.run_agent(request), preserving the typed outcome union.
// Parallelism belongs to the supplied dispatcher, not another task modifier.
// This scenario intentionally has no plain-call s11_Research_manual(...) wrapper.
function s11_Research_request(
  question: string,
  provider: root.ai.Provider = root.ai.FixtureTools {
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
    root.ai.prompt_template(`Research ${question}.`, false),
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

test "11: composed dispatch preserves owner batches and provider call order" {
  let audit: string[] = [];
  let task_dispatch = (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    audit.push("task:" + calls.length().to_string());
    calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
      root.ai.ToolResult.ok(call, "task")
    })
  };
  let agent_dispatch = (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
    audit.push("agent:" + calls.length().to_string());
    calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
      root.ai.ToolResult.ok(call, "agent")
    })
  };
  let dispatch = root.ai.compose_tool_dispatch(
    [s11_search_tool()],
    task_dispatch,
    [s11_lookup_tool()],
    agent_dispatch,
  );
  let results = dispatch([
    root.ai.ToolCall { id: "a", name: "lookup", args: `{"id":"1"}` },
    root.ai.ToolCall { id: "b", name: "search", args: `{"query":"Paris"}` },
    root.ai.ToolCall { id: "c", name: "lookup", args: `{"id":"2"}` },
  ]);
  assert.equal(audit, ["agent:2", "task:1"])
  assert.equal(results.map((result: root.ai.ToolResult) -> string { result.id }), ["a", "b", "c"])
  assert.equal(results.map((result: root.ai.ToolResult) -> string { result.output }), ["agent", "task", "agent"])
}

test "11: duplicate tool names are rejected before an agent runs" {
  let message = root.ai.merge_tools([s11_search_tool()], [s11_search_tool()]) catch (e) {
    let unsupported: baml.errors.Unsupported => unsupported.message,
    _ => "wrong error",
  };
  assert.equal(message, "duplicate tool name: search")
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
