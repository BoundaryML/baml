// 09 — the task has no `tools:` field. The roster and dispatcher are call-time data.

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s09_Weather(city: string) -> string {
//   client: root.ai.FakeToolProvider {
//     call_name: "get_weather",
//     call_args: `{"city":"Tokyo"}`,
//     final_json: `"It is cloudy and 18 C."`,
//   }
//   prompt: `What is the weather in ${city}?`
// }

// Manual expansion reference for s09_Weather.
// The helper names below are hand-written reference/test scaffolding, not
// literal compiler-generated companion names. `provider` stands in for the
// task's synthetic trailing `client =` argument.
// - s09_Weather_request(...) models the baseline s09_Weather.request(...)
//   without a call-time tool roster or dispatcher; it performs no I/O.
// - s09_Weather_agent_request(...) hand-builds that Request<string> after
//   folding in the s09_Weather.agent(...) roster and dispatcher.
// - s09_Weather_agent_manual(...) models s09_Weather.agent(...):
//   root.ai.run_agent(agent_request), preserving its typed outcome union.
// This scenario intentionally has no plain-call s09_Weather_manual(...) wrapper.
function s09_Weather_request(
  city: string,
  provider: root.ai.Provider = root.ai.FakeToolProvider {
    call_name: "get_weather",
    call_args: `{"city":"Tokyo"}`,
    final_json: `"It is cloudy and 18 C."`,
  },
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s09_Weather",
    "ns_ai_scenarios",
    { "city": city },
    [],
    root.ai.RequestOptions { dispatch: null },
    {},
    root.ai.prompt_template(`What is the weather in ${city}?`, false),
  )
}

function s09_Weather_agent_request(
  city: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider,
    "s09_Weather",
    "ns_ai_scenarios",
    { "city": city },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    root.ai.prompt_template(`What is the weather in ${city}?`, false),
  )
}

function s09_Weather_agent_manual(
  city: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s09_Weather_agent_request(city, provider, tools, dispatch))
}

// Manual expansion reference for a deferred side-effect-only
// `s09_MarkComplete(task_id: string) -> null` LLM function with tools supplied
// at call time. A non-tool assistant turn completes the loop; its text is
// intentionally discarded because the declared result carries no payload.
function s09_MarkComplete_agent_request(
  task_id: string,
  provider: root.ai.Provider,
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Request<null> {
  root.ai.task_request<null>(
    provider,
    "s09_MarkComplete",
    "ns_ai_scenarios",
    { "task_id": task_id },
    [s09_completion_tool()],
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    root.ai.prompt_template(
      `Call mark_complete for task ${task_id} with completed=true. After the tool succeeds, finish.`,
      false,
    ),
  )
}

function s09_MarkComplete_agent_manual(
  task_id: string,
  provider: root.ai.Provider,
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<null> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<null>(s09_MarkComplete_agent_request(task_id, provider, dispatch))
}

test "09: a call-time roster drives a typed tool loop" {
  let outcome = s09_Weather_agent_manual(
    "Tokyo",
    root.ai.FakeToolProvider {
      call_name: "get_weather",
      call_args: `{"city":"Tokyo"}`,
      final_json: `"It is cloudy and 18 C."`,
    },
    [s09_weather_tool()],
    s09_weather_dispatch,
  );
  match (outcome) {
    let done: root.ai.Done<string> => assert.is_true(done.value.includes("18")),
    _ => assert.is_true(false),
  }
}

test "09: malformed tool arguments become a ToolResult for self-correction" {
  let call = root.ai.ToolCall { id: "bad", name: "get_weather", args: `{"wrong":1}` };
  let output = s09_weather_dispatch([call]).at(0)?.output ?? "";
  assert.is_true(output.includes("error"))
}

testset "integ-test-09-tool-calling" {
  test "09 live: OpenAI drives the same manual request and call-time roster" {
    let seen: string[] = [];
    let dispatch = (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
      let out: root.ai.ToolResult[] = [];
      for (let call in calls) {
        let args: S09WeatherArgs = baml.json.from_string<S09WeatherArgs>(call.args) catch (e) {
          _ => S09WeatherArgs { city: "unknown" },
        };
        seen.push(args.city);
        out.push(root.ai.ToolResult { id: call.id, output: `cloudy, 18 C in ${args.city}` });
      }
      out
    };
    let outcome = s09_Weather_agent_manual(
      "Tokyo",
      root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
      [s09_weather_tool()],
      dispatch,
    );
    assert.is_true(seen.length() > 0)
    match (outcome) {
      let done: root.ai.Done<string> => assert.is_true(done.value.length() > 0),
      _ => assert.is_true(false),
    }
  }


  test "09 live: OpenAI strict completes a null-returning tool task" {
    let completed_ids: string[] = [];
    let dispatch = (calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] {
      calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
        let args: S09CompletionArgs = baml.json.from_string<S09CompletionArgs>(call.args) catch (e) {
          _ => return root.ai.ToolResult { id: call.id, output: "invalid arguments" },
        };
        if (args.completed) { completed_ids.push(args.task_id); }
        root.ai.ToolResult { id: call.id, output: "task marked complete" }
      })
    };
    let outcome = s09_MarkComplete_agent_manual(
      "task-42",
      root.ai.OpenAiStrict {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
        extra_headers: null,
        extra_body: null,
      },
      dispatch,
    );
    assert.equal(completed_ids, ["task-42"]);
    match (outcome) {
      let done: root.ai.Done<null> => assert.equal(done.value, null),
      _ => assert.is_true(false),
    }
  }
}
