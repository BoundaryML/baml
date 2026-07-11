// 42 — application code over one HarnessAgent facade.

class S42Patch {
  files_changed: string[],
  summary: string,
}

function s42_code_agent() -> root.ai.HarnessAgent throws never {
  root.ai.HarnessAgent {
    inner: S42ClaudeHarness {
      reply: `{"files_changed":["tests.baml"],"summary":"claude inspected tests"}`,
      next_id: 0,
      sandbox: "container",
    },
  }
}

function s42_pi_agent() -> root.ai.HarnessAgent throws never {
  root.ai.HarnessAgent {
    inner: S42PiHarness {
      reply: `{"files_changed":["pi.baml"],"summary":"pi inspected tests"}`,
      next_id: 0,
    },
  }
}

// Runtime selection is ordinary application code; downstream task code does not
// name Claude or Pi.
function s42_coder(runtime: string) -> root.ai.Provider throws never {
  if (runtime == "pi") { s42_pi_agent() } else { s42_code_agent() }
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s42_Summarize(task: string) -> S42Patch {
//   client: s42_code_agent()
//   prompt: `Inspect the repository and summarize the tests, then ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s42_Summarize`; these helper names are scaffolding, not literal generated names:
// - `s42_Summarize_request` represents `s42_Summarize.request(...)`.
// - `s42_Summarize_manual` spells the plain `s42_Summarize(...)` lowering
//   through `root.ai.run`.
function s42_Summarize_request(
  task: string,
  provider: root.ai.Provider = root.ai.HarnessAgent {
    inner: S42ClaudeHarness {
      reply: `{"files_changed":["tests.baml"],"summary":"claude inspected tests"}`,
      next_id: 0,
      sandbox: "container",
    },
  },
) -> root.ai.Request<S42Patch> throws never {
  root.ai.task_request<S42Patch>(
    provider,
    "s42_Summarize",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(
      `Inspect the repository and summarize the test setup, then: ${task}.`,
      true,
    ),
  )
}

function s42_Summarize_manual(
  task: string,
  provider: root.ai.Provider = root.ai.HarnessAgent {
    inner: S42ClaudeHarness {
      reply: `{"files_changed":["tests.baml"],"summary":"claude inspected tests"}`,
      next_id: 0,
      sandbox: "container",
    },
  },
) -> S42Patch throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.run<S42Patch>(s42_Summarize_request(task, provider = provider))
}

// Deferred source-task sketch:
// function s42_SummarizeStream(task: string) -> S42Patch {
//   client: s42_code_agent()
//   prompt: `Inspect the repo and summarize: ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s42_SummarizeStream`; these helper names are scaffolding, not literal generated names:
// - `s42_SummarizeStream_request` represents `s42_SummarizeStream.request(...)`.
// - `s42_SummarizeStream_manual` is this scenario's custom event-stream
//   consumer of that request, not a literal generated helper.
// - `s42_DriveRich_manual` is an application-level rich-UI consumer layered on it.
function s42_SummarizeStream_request(
  task: string,
  provider: root.ai.Provider = root.ai.HarnessAgent {
    inner: S42ClaudeHarness {
      reply: `{"files_changed":["stream.baml"],"summary":"streamed harness events"}`,
      next_id: 0,
      sandbox: "container",
    },
  },
) -> root.ai.Request<S42Patch> throws never {
  root.ai.task_request<S42Patch>(
    provider,
    "s42_SummarizeStream",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Inspect the repo and summarize: ${task}.`, true),
  )
}

function s42_SummarizeStream_manual(
  task: string,
  provider: root.ai.Provider = root.ai.HarnessAgent {
    inner: S42ClaudeHarness {
      reply: `{"files_changed":["stream.baml"],"summary":"streamed harness events"}`,
      next_id: 0,
      sandbox: "container",
    },
  },
) -> root.ai.AgentEventLog<S42Patch>
    throws baml.errors.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
  s42_events<S42Patch>(s42_SummarizeStream_request(task, provider = provider))
}

// Rich UI consumer: preserves tool/control cards instead of projecting to text.
function s42_DriveRich_manual(
  task: string,
  provider: root.ai.Provider,
) -> root.ai.AgentEventLog<S42Patch>
    throws baml.errors.CallError | baml.errors.UnknownError | baml.errors.Unsupported {
  s42_SummarizeStream_manual(task, provider = provider)
}

function s42_build_server(
  agent: root.ai.HarnessAgent,
  store: S42HandleStore,
) -> S42ServerSession throws never {
  S42ServerSession { agent: agent, store: store }
}

// POST /agent/start: first request creates, drives, detaches, and stores a handle.
function s42_HandleStart(
  spec: string,
  server: S42ServerSession,
) -> S42Turn<S42Patch> throws baml.errors.UnknownError | baml.errors.Unsupported {
  server.open<S42Patch>(
    s42_Summarize_request(spec, provider = server.agent),
    root.ai.SessionOptions {
      cwd: "/workspace",
      resume_from: null,
      permission_mode: "accept-edits",
      attributes: {},
    },
  )
}

// POST /agent/:id/continue: a later request reloads the serializable handle and
// attaches a new turn to the same provider-owned session.
function s42_HandleContinue(
  id: string,
  follow_up: string,
  server: S42ServerSession,
) -> S42Turn<S42Patch> throws baml.errors.UnknownError | baml.errors.Unsupported {
  server.resume<S42Patch>(
    id,
    s42_Summarize_request(follow_up, provider = server.agent),
  )
}

function s42_HandleEnd(id: string, server: S42ServerSession) -> null
    throws baml.errors.UnknownError | baml.errors.Unsupported {
  server.close(id)
}

// Lossy normalization is explicit: ask the facade for its raw provider, then
// negotiate the provider-specific verb. Claude/container can rewind; Pi cannot.
function s42_RewindIfPossible(agent: root.ai.HarnessAgent) -> bool throws never {
  match (agent.raw()) {
    let controls: root.ai.HarnessControlPlane => controls.supported_verbs().includes("rewind_files"),
    _ => false,
  }
}

// Canonical stream consumer used only by the live testset. HarnessAgent forwards
// it when its inner provider really implements root.ai.Streaming.
function s42_stream_live(
  task: string,
  provider: root.ai.Provider,
) -> S42Patch throws never {
  let stream = root.ai.stream<S42Patch$stream, S42Patch>(
    s42_SummarizeStream_request(task, provider = provider),
  ) catch (e) {
    _ => return S42Patch { files_changed: [], summary: "stream-open-error" },
  };
  while (true) {
    match (stream.next()) {
      let ignored: S42Patch$stream => {},
      let ignored: baml.stream.StreamFinished => { break; },
    } catch (e) {
      _ => return S42Patch { files_changed: [], summary: "stream-read-error" },
    }
  }
  stream.final() catch (e) {
    _ => S42Patch { files_changed: [], summary: "stream-final-error" },
  }
}

test "42: one hand-written task companion runs against Claude or Pi" {
  let claude = s42_Summarize_manual("note flaky tests", provider = s42_coder("claude"));
  let pi = s42_Summarize_manual("note flaky tests", provider = s42_coder("pi"));
  assert.equal(claude.summary, "claude inspected tests")
  assert.equal(pi.summary, "pi inspected tests")
}

test "42: the streaming task companion exposes a lossless rich event log" {
  let log = s42_SummarizeStream_manual("show tool work");
  assert.equal(root.ai.agent_run_value(log.run).summary, "streamed harness events")
  assert.equal((log.events[0]).kind(), "model_started")
  assert.equal((log.events[1]).kind(), "text_delta")
}

test "42: rich consumers use the same request and preserve tool cards" {
  let log = s42_DriveRich_manual("inspect tests", s42_code_agent());
  let has_tool = false;
  for (let event in log.events) {
    if (event.kind() == "tool_call_started") { has_tool = true; }
  }
  assert.is_true(has_tool)
}

test "42: a serializable handle continues one session across HTTP requests" {
  let store = S42HandleStore { handles: [] };
  let server = s42_build_server(s42_code_agent(), store);
  let first = s42_HandleStart("inspect the tests", server);
  let encoded = baml.json.to_string(first.token);
  let persisted = baml.json.from_string<root.ai.HarnessSessionToken>(encoded);
  assert.equal(persisted.runtime, "claude-code")
  assert.equal(first.run.conversation.messages.length(), 2)

  let second = s42_HandleContinue(first.token.session_id, "now find flaky cases", server);
  assert.equal(second.token.session_id, first.token.session_id)
  assert.is_true(second.token.sealed != first.token.sealed)
  assert.equal(second.value.summary, "claude inspected tests")
}

test "42: the server owns expiry and teardown truth" {
  let store = S42HandleStore { handles: [] };
  let server = s42_build_server(s42_pi_agent(), store);
  let first = s42_HandleStart("inspect", server);
  s42_HandleEnd(first.token.session_id, server);
  let result = s42_HandleContinue(first.token.session_id, "continue", server) catch (e) {
    _ => "expired",
  };
  let label = match (result) {
    let turn: S42Turn<S42Patch> => turn.token.session_id,
    let text: string => text,
  };
  assert.equal(label, "expired")
}

test "42: runtime feature parity leaks through one documented escape hatch" {
  assert.is_true(s42_RewindIfPossible(s42_code_agent()))
  assert.is_true(!s42_RewindIfPossible(s42_pi_agent()))
  assert.is_true(s42_code_agent().raw() is root.ai.HarnessControlPlane)
  assert.is_true(s42_pi_agent().raw() is root.ai.HarnessControlPlane)
}

test "42: usage aggregates across model turns independently of events" {
  let first = root.ai.Meta {
    provider: "fake", model: "m", request_id: "1", finish_reason: "tool_calls",
    usage: root.ai.Usage {
      input_tokens: 10, output_tokens: 3, cached_input_tokens: 4,
      reasoning_tokens: 2, cost_usd: 0.01,
    },
    attributes: {}, raw: null,
  };
  let second = root.ai.Meta {
    provider: "fake", model: "m", request_id: "2", finish_reason: "stop",
    usage: root.ai.Usage {
      input_tokens: 20, output_tokens: 7, cached_input_tokens: 0,
      reasoning_tokens: 5, cost_usd: 0.02,
    },
    attributes: {}, raw: null,
  };
  let usage = root.ai.aggregate_usage([first, second]);
  assert.equal(usage?.input_tokens, 30)
  assert.equal(usage?.output_tokens, 10)
  assert.equal(usage?.reasoning_tokens, 7)
  assert.equal(usage?.cost_usd, 0.03)
}

test "42: a HarnessAgent may also wrap a plain Generate provider" {
  let agent: root.ai.Provider = root.ai.HarnessAgent {
    inner: root.ai.FakeGenerateProvider {
      name: "plain-model",
      reply: `{"files_changed":[],"summary":"plain generate"}`,
      attributes: {},
    },
  };
  let patch = s42_Summarize_manual("inspect", provider = agent);
  assert.equal(patch.summary, "plain generate")
}

testset "integ-test-42-harness-abstraction" {
  test "42 live: HarnessAgent forwards Generate to an OpenAI inner provider" {
    let agent: root.ai.Provider = root.ai.HarnessAgent {
      inner: root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    };
    let patch = s42_Summarize_manual(
      "Return an empty files_changed list and a one-line summary.",
      provider = agent,
    );
    assert.is_true(patch.summary.length() > 0)
  }

  test "42 live: the same HarnessAgent forwards canonical Streaming" {
    let agent: root.ai.Provider = root.ai.HarnessAgent {
      inner: root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    };
    let patch = s42_stream_live(
      "Return an empty files_changed list and a one-line summary.",
      agent,
    );
    assert.is_true(patch.summary.length() > 0)
  }
}
