// 39 — application-author reference for tools, MCP, hooks, skills,
// sub-agents, realtime, and A2A peers.

function s39_default_harness() -> root.ai.Provider throws never {
  s39_ScriptedHarness {
    calls: [],
    final_json: `{"files_changed":[],"summary":"done"}`,
    skill_json: `{"severity":"high","reproducible":true,"summary":"auth handler 500s"}`,
    skill_commands: ["triage"],
    submitted: [],
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s39_Fix(task: string) -> s39_Patch {
//   client: s39_default_harness()
//   prompt: `Fix the bug, then ask the code-reviewer sub-agent to check it: ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for `s39_Fix`.
// These helper names are scaffolding, not literal generated names:
// - `s39_Fix_request` represents `s39_Fix.request(...)` with call-time tools.
// - `s39_Fix_manual` spells the explicit agent-loop consumer of that request.
function s39_Fix_request(
  task: string,
  provider: root.ai.Provider = s39_default_harness(),
  tools: root.ai.Tool[] = [],
  dispatch: ((root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never)? = null,
) -> root.ai.Request<s39_Patch> {
  root.ai.task_request<s39_Patch>(
    provider,
    "s39_Fix",
    "ns_ai_scenarios",
    { "task": task },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    { "scenario": "39" },
    s39_prompt_template(
      `Fix the bug, then ask the code-reviewer sub-agent to check it: ${task}.`,
      true,
    ),
  )
}

function s39_Fix_manual(
  task: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<s39_Patch> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<s39_Patch>(s39_Fix_request(
    task,
    provider = provider,
    tools = tools,
    dispatch = dispatch,
  ))
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s39_TriageIssue(issue_number: int) -> s39_Triage {
//   client: s39_default_harness()
//   prompt: `Run the triage skill for issue ${issue_number}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s39_TriageIssue`; these helper names are scaffolding, not literal generated names:
// - `s39_TriageIssue_request` represents `s39_TriageIssue.request(...)`.
// - `s39_TriageIssue_manual` is a custom Skills-capability consumer of that
//   request, not a compiler-generated companion.
function s39_TriageIssue_request(
  issue_number: int,
  provider: root.ai.Provider = s39_default_harness(),
) -> root.ai.Request<s39_Triage> {
  root.ai.task_request<s39_Triage>(
    provider,
    "s39_TriageIssue",
    "ns_ai_scenarios",
    { "issue_number": issue_number },
    [],
    root.ai.RequestOptions {},
    { "scenario": "39", "mode": "skill" },
    s39_prompt_template(`Run the triage skill for issue ${issue_number}.`, true),
  )
}

function s39_TriageIssue_manual(
  issue_number: int,
  provider: root.ai.Provider = s39_default_harness(),
) -> s39_Triage throws baml.errors.UnknownError {
  let request = s39_TriageIssue_request(issue_number, provider = provider);
  match (provider) {
    let skills: s39_Skills => skills.run_skill<s39_Triage>(
      request,
      "triage",
      { "issue_number": issue_number },
    ),
    _ => throw baml.errors.UnknownError {
      data: provider,
      message: ["provider does not expose the Skills capability"],
    },
  }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s39_Pair(task: string) -> string {
//   client: s39_default_harness()
//   prompt: `Pair with me on: ${task}`
// }

// Hand-written reference expansion of the compiler task surface for `s39_Pair`.
// These helper names are scaffolding, not literal generated names:
// - `s39_Pair_request` represents `s39_Pair.request(...)`.
// - `s39_Pair_manual` is the explicit live-channel consumer of that request.
function s39_Pair_request(
  task: string,
  provider: root.ai.Provider = s39_default_harness(),
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s39_Pair",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    { "scenario": "39", "mode": "live" },
    s39_prompt_template(`Pair with me on: ${task}`, false),
  )
}

function s39_Pair_manual(
  task: string,
  channel: root.ai.Channel,
  provider: root.ai.Provider = s39_default_harness(),
) -> root.ai.Live throws baml.errors.UnknownError {
  root.ai.open_live<string>(s39_Pair_request(task, provider = provider), channel)
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s39_Summarize(doc: string) -> string {
//   client: s39_A2aProvider { endpoint: null, headers: {}, fixture_text: `"short"` }
//   prompt: `Summarize: ${doc}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s39_Summarize`; these helper names are scaffolding, not literal generated names:
// - `s39_Summarize_request` represents `s39_Summarize.request(...)`.
// - `s39_Summarize_manual` spells the plain `s39_Summarize(...)` lowering
//   through `root.ai.run`.
function s39_Summarize_request(
  doc: string,
  provider: root.ai.Provider = s39_A2aProvider {
    endpoint: null,
    headers: {},
    fixture_text: `"short"`,
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s39_Summarize",
    "ns_ai_scenarios",
    { "doc": doc },
    [],
    root.ai.RequestOptions {},
    { "scenario": "39", "transport": "a2a" },
    s39_prompt_template(`Summarize: ${doc}`, false),
  )
}

function s39_Summarize_manual(
  doc: string,
  provider: root.ai.Provider = s39_A2aProvider {
    endpoint: null,
    headers: {},
    fixture_text: `"short"`,
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<string>(s39_Summarize_request(doc, provider = provider))
}

class s39_NullChannel {
  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io { null }
    function close(self) -> null throws never { null }
  }
}

test "39: local and MCP tools share one roster and one hook-gated dispatcher" {
  let audit: string[] = [];
  let tools = [s39_weather_tool(), s39_mcp_issue_tool(), s39_builtin_tool("Read")];
  let dispatch = s39_dispatch(tools, [s39_AuditHook { events: audit }]);
  let results = dispatch([
    root.ai.ToolCall {
      id: "local-1",
      name: "get_temperature",
      args: `{"latitude":52.5,"longitude":13.4}`,
    },
    root.ai.ToolCall { id: "mcp-1", name: "mcp__github__search_issues", args: `{"query":"login"}` },
  ]);
  assert.equal(results.at(0)?.output, "12.5C @ 52.5,13.4")
  assert.equal(results.at(1)?.output, "issue 42: login returns 500")
  assert.equal(audit, [
    "pre_tool:get_temperature", "pre_tool:mcp__github__search_issues",
    "post_tool:get_temperature", "post_tool:mcp__github__search_issues",
  ])
}

test "39: a blocking hook returns a correlated tool error" {
  let result = s39_dispatch(
    [s39_builtin_tool("Bash")],
    [s39_BlockToolHook { blocked_name: "Bash" }],
  )([root.ai.ToolCall { id: "danger", name: "Bash", args: `{"command":"rm -rf /"}` }]);
  assert.equal(result.at(0)?.id, "danger")
  assert.is_true((result.at(0)?.output ?? "").includes("blocked by policy"))
}

test "39: the handwritten Fix companion drives the shared Tools loop" {
  let rich_tools = [s39_weather_tool(), s39_mcp_issue_tool()];
  let harness = s39_ScriptedHarness {
    calls: [
      root.ai.ToolCall {
        id: "local-1", name: "get_temperature",
        args: `{"latitude":52.5,"longitude":13.4}`,
      },
      root.ai.ToolCall { id: "mcp-1", name: "mcp__github__search_issues", args: `{"query":"login"}` },
    ],
    final_json: `{"files_changed":["auth.baml"],"summary":"fixed and reviewed"}`,
    skill_json: `{"severity":"high","reproducible":true,"summary":"login 500"}`,
    skill_commands: ["triage"],
    submitted: [],
  };
  let outcome = s39_Fix_manual(
    "login returns 500",
    harness,
    s39_tool_specs(rich_tools),
    s39_dispatch(rich_tools, []),
  );
  match (outcome) {
    let done: root.ai.Done<s39_Patch> => assert.equal(done.value.summary, "fixed and reviewed"),
    _ => assert.is_true(false),
  }
  assert.equal(harness.submitted, ["12.5C @ 52.5,13.4", "issue 42: login returns 500"])
}

test "39: shared observers record lifecycle events without becoming policy" {
  let harness = s39_ScriptedHarness {
    calls: [root.ai.ToolCall {
      id: "local-1",
      name: "get_temperature",
      args: `{"latitude":52.5,"longitude":13.4}`,
    }],
    final_json: `{"files_changed":[],"summary":"observed"}`,
    skill_json: `{}`,
    skill_commands: [],
    submitted: [],
  };
  let tools = [s39_weather_tool()];
  let observer = s39_EventAudit { kinds: [] };
  let recorder = s39_EventAudit { kinds: [] };
  let request = s39_Fix_request(
    "observe the run",
    provider = harness,
    tools = s39_tool_specs(tools),
    dispatch = s39_dispatch(tools, []),
  ).with_observers([observer]).with_recorders([recorder]);
  let run = root.ai.run_agent_detailed<s39_Patch>(request);
  assert.equal(root.ai.agent_run_value(run).summary, "observed")
  assert.is_true(observer.kinds.includes("tool_call_started"))
  assert.is_true(observer.kinds.includes("run_finished"))
  assert.is_true(recorder.kinds.includes("record:tool_call_finished"))
  assert.equal(run.turns.length(), 2)
}

test "39: provider blocks extend the shared method-only interface out of body" {
  let block: root.ai.ModelBlock = s39_ThinkingBlock {
    digest: "considered two patches",
    signature_hint: "provider-owned",
  };
  assert.equal(block.kind(), "s39_thinking")
  assert.equal(block.provider(), "example-provider")
}

test "39: a skill is a distinct typed capability over the same Request<T>" {
  let result = s39_TriageIssue_manual(42);
  assert.equal(result.severity, "high")
  assert.is_true(result.reproducible)
}

test "39: the live companion returns a resource and the caller owns cleanup" {
  let live = s39_Pair_manual("refactor auth", s39_NullChannel {});
  defer { live.cleanup() }
  assert.equal(live.events().length(), 3)
}

test "39: a SubAgent forwards Generate but withholds nested extension capabilities" {
  let child: root.ai.Provider = s39_SubAgent {
    inner: s39_A2aProvider { endpoint: null, headers: {}, fixture_text: `"reviewed"` },
    description: "Review only; no nested agents.",
  };
  assert.equal(s39_Summarize_manual("patch", provider = child), "reviewed")
  let shape = match (child) { let tools: root.ai.Tools => "tools", _ => "generate-only" };
  assert.equal(shape, "generate-only")
}

test "39: an A2A peer is portable behind the ordinary Generate driver" {
  assert.equal(s39_Summarize_manual("a long document"), "short")
}

testset "integ-test-39-harness-extensibility" {
  test "39 live: the A2A request companion speaks JSON-RPC to a discovered peer" {
    let peer = s39_A2aProvider {
      endpoint: baml.env.get_or_panic("A2A_AGENT_URL"),
      headers: { "authorization": "Bearer " + baml.env.get_or_panic("A2A_TOKEN") },
      fixture_text: null,
    };
    let answer = s39_Summarize_manual("Summarize this integration probe.", provider = peer);
    assert.is_true(answer.length() > 0)
  }

  test "39 live: the same request companion can swap to OpenAI" {
    let answer = s39_Summarize_manual(
      "BEPv2 makes requests first-class.",
      provider = root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(answer.length() > 0)
  }
}
