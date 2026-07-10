// Scenario 38 application surface.
class s38_FixResult { summary: string }
class s38_ReadArgs { path: string }
class s38_WriteArgs { path: string, contents: string }
class s38_BashArgs { command: string }

// Deferred source sketch:
// function s38_FixTask(task: string) -> s38_FixResult {
//   client: s38_read_only_harness([], `{"summary":"no-op"}`)
//   prompt: `Fix the task: ${task}. ${ctx.output_format}`
// }

// This is the desugared code for s38_FixTask ----------------------------
function s38_FixTask_request(
  task: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Request<s38_FixResult> throws never {
  root.ai.task_request<s38_FixResult>(
    provider,
    "s38_FixTask",
    "ns_ai_scenarios",
    { "task": task },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    bepv2_prompt_template("Fix the task, using tools as needed: " + task, true),
  )
}

function s38_FixTask_agent_manual(
  task: string,
  provider: root.ai.Provider,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
) -> root.ai.Done<s38_FixResult> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<s38_FixResult>(
    s38_FixTask_request(task, provider, tools, dispatch),
    budget = null,
  )
}

function s38_code_tools() -> root.ai.Tool[] throws never {
  [
    root.ai.Tool.from_type("read_file", "Read a file", reflect.type_of<s38_ReadArgs>()),
    root.ai.Tool.from_type("write_file", "Write a file", reflect.type_of<s38_WriteArgs>()),
    root.ai.Tool.from_type("bash", "Run a command inside the configured sandbox", reflect.type_of<s38_BashArgs>()),
  ]
}

function s38_read_call() -> root.ai.ToolCall throws never {
  root.ai.ToolCall { id: "read-1", name: "read_file", args: `{"path":"src/main.rs"}` }
}

function s38_write_call() -> root.ai.ToolCall throws never {
  root.ai.ToolCall {
    id: "write-1",
    name: "write_file",
    args: `{"path":"src/main.rs","contents":"fixed"}`,
  }
}

function s38_bash_call() -> root.ai.ToolCall throws never {
  root.ai.ToolCall { id: "bash-1", name: "bash", args: `{"command":"cargo check"}` }
}

function s38_gate_status(
  call: root.ai.ToolCall,
  mode: string,
  allowed_tools: string[],
  denied_tools: string[],
  sandbox: s38_Sandbox,
  approve: ((root.ai.ToolCall) -> s38_ApprovalDecision throws never)?,
) -> string throws never {
  let results = s38_gate([call], mode, allowed_tools, denied_tools, sandbox, approve) catch (e) {
    let denied: s38_PermissionDenied => return "denied:" + denied.tool,
  };
  (results[0]).output
}

test "38: read-only permits reads and denies writes" {
  let sandbox = s38_VirtualSandbox {};
  assert.equal(
    s38_gate_status(s38_read_call(), "read-only", ["read_file", "write_file"], [], sandbox, null),
    "read ok",
  )
  assert.equal(
    s38_gate_status(s38_write_call(), "read-only", ["read_file", "write_file"], [], sandbox, null),
    "denied:write_file",
  )
}

test "38: ask mode delegates each write to the call-time approval hook" {
  let sandbox = s38_VirtualSandbox {};
  assert.equal(
    s38_gate_status(s38_write_call(), "ask", ["write_file"], [], sandbox, s38_allow()),
    "wrote ok",
  )
  assert.equal(
    s38_gate_status(s38_write_call(), "ask", ["write_file"], [], sandbox, s38_deny()),
    "denied:write_file",
  )
  assert.equal(
    s38_gate_status(s38_write_call(), "ask", ["write_file"], [], sandbox, null),
    "denied:write_file",
  )
}

test "38: an explicit deny list wins before a permissive callback" {
  assert.equal(
    s38_gate_status(s38_write_call(), "ask", ["write_file"], ["write_file"], s38_VirtualSandbox {}, s38_allow()),
    "denied:write_file",
  )
}

test "38: the sandbox is the command-execution fence" {
  let virtual = s38_gate_status(
    s38_bash_call(),
    "full-auto",
    ["bash"],
    [],
    s38_VirtualSandbox {},
    null,
  );
  let container = s38_gate_status(
    s38_bash_call(),
    "full-auto",
    ["bash"],
    [],
    s38_ContainerSandbox { image: "baml-test:latest" },
    null,
  );
  assert.is_true(virtual.starts_with("virtual:"))
  assert.is_true(container.starts_with("container:"))
}

test "38: sandbox and permission posture are bound harness config" {
  let read_only = s38_read_only_harness([], `{"summary":"no-op"}`);
  assert.equal(read_only.permission_mode, "read-only")
  assert.equal(read_only.sandbox.label(), "virtual")
  assert.is_true(read_only.allowed_tools.includes("read_file"))

  let full_auto = s38_full_auto_harness([], `{"summary":"no-op"}`);
  assert.equal(full_auto.permission_mode, "full-auto")
  assert.is_true(full_auto.sandbox.label().starts_with("container:"))
  assert.is_true(full_auto.allowed_tools.includes("bash"))
}

test "38: a denial is a refusal known not to have committed" {
  let failure: root.ai.Failure = s38_PermissionDenied {
    tool: "write_file",
    mode: "read-only",
    reason: "mode forbids writes",
  };
  assert.equal(failure.kind(), root.ai.FailureKind.Refusal)
  assert.equal(failure.commit_state(), root.ai.CommitState.NotCommitted)
  assert.is_true(!root.ai.may_replay(
    root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Safe, idempotency_key: null },
    failure,
  ))
}

test "38: the non-throwing dispatcher encodes permission refusal as loop data" {
  let dispatch = s38_permissioned_dispatch(
    "read-only",
    ["read_file", "write_file"],
    [],
    s38_VirtualSandbox {},
    null,
  );
  let results = dispatch([s38_write_call()]);
  assert.is_true((results[0]).output.includes("DENIED (read-only)"))
}

test "38: the handwritten agent companion uses the harness posture at call time" {
  let harness = s38_read_only_harness(
    [s38_write_call()],
    `{"summary":"reported the denied write"}`,
  );
  let outcome = s38_FixTask_agent_manual(
    "edit the config",
    harness,
    s38_code_tools(),
    s38_permissioned_dispatch(
      harness.permission_mode,
      harness.allowed_tools,
      harness.denied_tools,
      harness.sandbox,
      null,
    ),
  );
  match (outcome) {
    let done: root.ai.Done<s38_FixResult> => assert.equal(done.value.summary, "reported the denied write"),
    _ => assert.is_true(false),
  }
}

test "38: full-auto runs read and write calls inside the configured fence" {
  let harness = s38_full_auto_harness(
    [s38_read_call(), s38_write_call()],
    `{"summary":"applied the fix"}`,
  );
  let outcome = s38_FixTask_agent_manual(
    "read then fix",
    harness,
    s38_code_tools(),
    s38_permissioned_dispatch(
      harness.permission_mode,
      harness.allowed_tools,
      harness.denied_tools,
      harness.sandbox,
      null,
    ),
  );
  match (outcome) {
    let done: root.ai.Done<s38_FixResult> => assert.equal(done.value.summary, "applied the fix"),
    _ => assert.is_true(false),
  }
}

testset "integ-test-38-harness-permissions" {
  test "38 live: full-auto dispatch executes real model tool requests through the sandbox fence" {
    let provider: root.ai.Provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
    };
    let outcome = s38_FixTask_agent_manual(
      "Call read_file for src/main.rs, then return a one-sentence summary.",
      provider,
      s38_code_tools(),
      s38_permissioned_dispatch(
        "full-auto",
        ["read_file", "write_file", "bash"],
        [],
        s38_ContainerSandbox { image: "baml-test:latest" },
        null,
      ),
    );
    match (outcome) {
      let done: root.ai.Done<s38_FixResult> => assert.is_true(done.value.summary.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
