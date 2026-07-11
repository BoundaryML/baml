class s24_WeatherArgs { city: string }

// Task sketch: s24_WeatherVoice(question) -> string.

function s24_tools() -> root.ai.Tool[] throws never {
  [root.ai.Tool.typed<s24_WeatherArgs>("get_weather", "Get weather for a city.")]
}

function s24_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  calls.map((call) -> { root.ai.invoke_tool<s24_WeatherArgs>(call, (args) -> { `sunny in ${args.city}` }) })
}

// Manual expansion reference for task `s24_WeatherVoice`:
// - `s24_WeatherVoice_request(...)` models
//   `s24_WeatherVoice.request(...)`, with the call-time tool roster and
//   dispatcher written into the request.
// - `s24_WeatherVoice_agent_manual(...)` models
//   `s24_WeatherVoice.agent(...)`, lowered through `root.ai.run_agent` over
//   that request.
// These helper names are hand-written reference/test scaffolding, not literal
// compiler-generated names.
// No task `tools:` field is used. The call-time roster and dispatcher are
// written into the Request that the ordinary run_agent driver consumes.
function s24_WeatherVoice_request(
  question: string,
  provider: root.ai.Provider = s24_LiveTools {},
  tools: root.ai.Tool[] = s24_tools(),
) -> root.ai.Request<string> {
  root.ai.task_request<string>(
    provider, "s24_WeatherVoice", "ns_ai_scenarios", { "question": question },
    tools, root.ai.RequestOptions { dispatch: s24_dispatch }, {}, bepv2_prompt_template(question, false),
  )
}

function s24_WeatherVoice_agent_manual(
  question: string,
  provider: root.ai.Provider = s24_LiveTools {},
) -> root.ai.Done<string> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<string>(s24_WeatherVoice_request(question, provider = provider))
}

test "24: a mid-session tool result is threaded into the continued answer" {
  let outcome = s24_WeatherVoice_agent_manual("What is the weather?");
  match (outcome) {
    let done: root.ai.Done<string> => assert.contains(done.value, "sunny in Paris"),
    _ => assert.is_true(false),
  }
}

test "24: tool arguments are validated against their BAML type" {
  let good = root.ai.ToolCall { id: "1", name: "get_weather", args: `{"city":"Berlin"}` };
  let bad = root.ai.ToolCall { id: "2", name: "get_weather", args: `{"city":42}` };
  let tool = s24_tools().at(0);
  assert.is_true(tool?.validate_args(good) ?? false)
  assert.is_true(!(tool?.validate_args(bad) ?? true))
}

test "24: the task owns no tools; the manual request supplies them at call time" {
  let request = s24_WeatherVoice_request("weather?");
  assert.equal(request.tools.length(), 1)
  assert.not_null(request.options.dispatch)
}

// Live realtime tool events remain blocked until the reference Live resource
// exposes provider-native function-call events. The synchronous Tools lifecycle
// above keeps the same call IDs, typed arguments, and submit ordering.
