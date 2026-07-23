// 41 — application-facing deployment examples.
//
// Tools are intentionally absent from the task declarations. Per the current
// experiment, a harness receives its tools at call time or from provider config.

class S41Patch {
  files_changed: string[],
  summary: string,
}

// Deferred source-task sketch; compiler desugaring is intentionally disabled:
// function s41_Fix(task: string) -> S41Patch {
//   client: S41ClaudeJsonl { ... }
//   prompt: `Fix the repository task: ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for `s41_Fix`.
// These helper names are scaffolding, not literal generated names:
// - `s41_Fix_request` represents `s41_Fix.request(...)`.
// - `s41_Fix_manual` is this scenario's custom Generate-or-Drivable consumer
//   of that request; it is not the compiler's plain-call lowering.
function s41_Fix_request(
  task: string,
  provider: root.ai.Provider = S41ClaudeJsonl {
    final_json: `{"files_changed":["auth.baml"],"summary":"fixed auth"}`,
    busy_ids: [],
  },
) -> root.ai.Request<S41Patch> throws never {
  root.ai.task_request<S41Patch>(
    provider,
    "s41_Fix",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Inspect the repository and fix this task: ${task}.`, true),
  )
}

function s41_Fix_manual(
  task: string,
  provider: root.ai.Provider = S41ClaudeJsonl {
    final_json: `{"files_changed":["auth.baml"],"summary":"fixed auth"}`,
    busy_ids: [],
  },
) -> S41Patch throws baml.errors.CallError | baml.errors.UnknownError {
  s41_run<S41Patch>(s41_Fix_request(task, provider = provider))
}

// Deferred source-task sketch:
// function s41_FixViaRpc(task: string) -> S41Patch {
//   client: S41PiRpc { ... }
//   prompt: `Fix via the RPC coding agent: ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s41_FixViaRpc`; these helper names are scaffolding, not literal generated names:
// - `s41_FixViaRpc_request` represents `s41_FixViaRpc.request(...)`.
// - `s41_FixViaRpc_manual` is this scenario's custom Generate-or-Drivable
//   consumer of that request; it is not the compiler's plain-call lowering.
function s41_FixViaRpc_request(
  task: string,
  provider: root.ai.Provider = S41PiRpc {
    final_json: `{"files_changed":["rpc.baml"],"summary":"fixed over rpc"}`,
  },
) -> root.ai.Request<S41Patch> throws never {
  root.ai.task_request<S41Patch>(
    provider,
    "s41_FixViaRpc",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Fix via the RPC coding agent: ${task}.`, true),
  )
}

function s41_FixViaRpc_manual(
  task: string,
  provider: root.ai.Provider = S41PiRpc {
    final_json: `{"files_changed":["rpc.baml"],"summary":"fixed over rpc"}`,
  },
) -> S41Patch throws baml.errors.CallError | baml.errors.UnknownError {
  s41_run<S41Patch>(s41_FixViaRpc_request(task, provider = provider))
}

// Deferred source-task sketch:
// function s41_Translate(text: string, language: string) -> string {
//   client: S41RemoteAgent { ... }
//   prompt: `Translate ${text} to ${language}`
// }

// Hand-written reference expansion of the compiler task surface for
// `s41_Translate`; these helper names are scaffolding, not literal generated names:
// - `s41_Translate_request` represents `s41_Translate.request(...)`.
// - `s41_Translate_manual` is this scenario's custom Generate-or-Drivable
//   consumer of that request; it is not the compiler's plain-call lowering.
function s41_Translate_request(
  text: string,
  language: string,
  provider: root.ai.Provider = S41RemoteAgent {
    endpoint: "https://agents.example.test",
    deployment: "translator",
    reply: `"bonjour"`,
  },
) -> root.ai.Request<string> throws never {
  root.ai.task_request<string>(
    provider,
    "s41_Translate",
    "ns_ai_scenarios",
    { "text": text, "language": language },
    [],
    root.ai.RequestOptions {},
    {},
    root.ai.prompt_template(`Translate the following text to ${language}: ${text}`, false),
  )
}

function s41_Translate_manual(
  text: string,
  language: string,
  provider: root.ai.Provider = S41RemoteAgent {
    endpoint: "https://agents.example.test",
    deployment: "translator",
    reply: `"bonjour"`,
  },
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  s41_run<string>(s41_Translate_request(text, language, provider = provider))
}

function s41_translator_agent(inner: root.ai.Provider) -> S41DeployedAgent throws never {
  S41DeployedAgent {
    name: "translator",
    inner: inner,
    triggers: [
      S41Trigger { kind: "webhook", route: "/translate", schedule: null },
      S41Trigger { kind: "cron", route: null, schedule: "0 9 * * *" },
    ],
  }
}

function s41_on_webhook(
  customer_id: string,
  text: string,
  language: string,
  deployment: S41DeployedAgent,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
  deployment.invoke<string>(
    "customer:" + customer_id,
    s41_Translate_request(text, language, provider = deployment),
  )
}

test "41: the three host protocols implement one Drivable request contract" {
  let jsonl = s41_Fix_manual(
    "repair auth",
    provider = S41ClaudeJsonl {
      final_json: `{"files_changed":["auth.baml"],"summary":"jsonl"}`,
      busy_ids: [],
    },
  );
  let rpc = s41_Fix_manual(
    "repair auth",
    provider = S41PiRpc {
      final_json: `{"files_changed":["rpc.baml"],"summary":"rpc"}`,
    },
  );
  let gen_patch = s41_Fix_manual(
    "repair auth",
    provider = S41AgentsGenerator {
      final_json: `{"files_changed":["agent.py"],"summary":"generator"}`,
    },
  );
  assert.equal(jsonl.summary, "jsonl")
  assert.equal(rpc.summary, "rpc")
  assert.equal(gen_patch.summary, "generator")
}

test "41: every task test calls the hand-written task-surface helper" {
  let patch = s41_FixViaRpc_manual("fix the rpc adapter");
  let translation = s41_Translate_manual("hello", "French");
  assert.equal(patch.files_changed.at(0), "rpc.baml")
  assert.equal(translation, "bonjour")
}

test "41: registry settings are ordinary runtime boot data" {
  let registry = S41Registry { entries: [] };
  registry.register("pi", S41ProviderSettings {
    base_url: "http://pi.internal",
    api_key: "runtime-secret",
    headers: { "x-tenant": "acme" },
  });
  registry.register("pi", S41ProviderSettings {
    base_url: "http://pi-v2.internal",
    api_key: "rotated-secret",
    headers: null,
  });
  let settings = registry.resolve("pi");
  assert.equal(settings?.base_url, "http://pi-v2.internal")
  assert.equal(registry.resolve("missing"), null)
}

test "41: durable attachment enforces one writer per instance" {
  let durable = S41ClaudeJsonl {
    final_json: `{"files_changed":[],"summary":"done"}`,
    busy_ids: [],
  };
  let first = durable.attach("customer:42");
  let second = durable.attach("customer:42") catch (e) {
    _ => "busy",
  };
  let outcome = match (second) {
    let instance: S41Instance => instance.id,
    let label: string => label,
  };
  assert.equal(outcome, "busy")
  durable.release(first)
  assert.equal(durable.attach("customer:42").id, "customer:42")
}

test "41: deployment binds triggers and a durable instance key" {
  let harness = S41ClaudeJsonl {
    final_json: `"hola"`,
    busy_ids: [],
  };
  let deployment = s41_translator_agent(harness);
  let answer = s41_on_webhook("c-7", "hello", "Spanish", deployment);
  assert.equal(answer, "hola")
  assert.equal(deployment.triggers.length(), 2)
  assert.equal(deployment.triggers.at(0)?.route, "/translate")
  assert.equal(harness.busy_ids.length(), 0)
}

test "41: supervision preserves the Drivable capability" {
  let supervised: root.ai.Provider = S41SubprocessSupervisor {
    inner: S41PiRpc {
      final_json: `{"files_changed":["supervisor.baml"],"summary":"restarted"}`,
    },
    max_restarts: 2,
  };
  let patch = s41_Fix_manual("recover the child", provider = supervised);
  assert.equal(patch.summary, "restarted")
}

test "41: a transport adapter plugs into the shared Harness lifecycle" {
  let adapter: root.ai.Provider = S41HarnessAdapter {
    inner: S41PiRpc {
      final_json: `{"files_changed":["shared.baml"],"summary":"shared harness"}`,
    },
    next_id: 0,
  };
  let request = s41_Fix_request("use the common harness", provider = adapter);
  let run = root.ai.run_harness<S41Patch>(request);
  assert.equal(root.ai.agent_run_value(run).summary, "shared harness")
  assert.equal(run.session_token?.runtime, "pi-json-rpc")
  assert.equal(run.turns.length(), 1)
}

test "41: the remote deployment is just another Generate provider" {
  let response = s41_Translate_manual(
    "hello",
    "French",
    provider = S41RemoteAgent {
      endpoint: "https://agents.example.test",
      deployment: "translation-v2",
      reply: `"salut"`,
    },
  );
  assert.equal(response, "salut")
}

testset "integ-test-41-harness-deployment" {
  test "41 live: the same hand-written request runs through a deployed-compatible provider" {
    let patch = s41_Fix_manual(
      "Return exactly one file named auth.baml and a short summary.",
      provider = root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(patch.summary.length() > 0)
  }

  test "41 live: a consumer can swap the remote deployment for OpenAI" {
    let answer = s41_Translate_manual(
      "hello",
      "French",
      provider = root.ai.OpenAi {
        model: "gpt-5.6-luna",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        base_url: null,
      },
    );
    assert.is_true(answer.to_lower_case().includes("bonjour"))
  }
}
