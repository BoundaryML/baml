// Scenario 37 application surface. The declarative source is intentionally a
// comment: compiler desugaring is deferred until the scenarios prove the API.
class s37_Patch {
  files_changed: string[],
  summary: string,
}

class s37_EditArgs { path: string }

// Deferred source sketch:
// function s37_Fix(task: string) -> s37_Patch {
//   client: s37_claude_harness()
//   prompt: `Fix the bug: ${task}. ${ctx.output_format}`
// }

// Hand-written reference expansion of the compiler task surface for `s37_Fix`.
// These helper names are scaffolding, not literal generated names:
// - `s37_Fix_request` represents `s37_Fix.request(...)`.
// - `s37_Fix_live_manual` and `s37_Fix_drive_manual` are custom live/drive
//   consumers of that request; they are not compiler-generated companions.
// - `s37_Fix_agent_request` binds call-time tools to the same task request, and
//   `s37_Fix_agent_manual` spells its explicit `root.ai.run_agent` lowering.
function s37_Fix_request(
  task: string,
  provider: root.ai.Provider = s37_claude_harness(),
) -> root.ai.Request<s37_Patch> throws never {
  root.ai.task_request<s37_Patch>(
    provider,
    "s37_Fix",
    "ns_ai_scenarios",
    { "task": task },
    [],
    root.ai.RequestOptions {},
    {},
    bepv2_prompt_template("Fix the bug: " + task, true),
  )
}

// The harness has no Generate path. Its one-shot form is the task's live
// companion over a throwaway or driver-owned channel.
function s37_Fix_live_manual(
  task: string,
  channel: root.ai.Channel,
  provider: root.ai.Provider = s37_claude_harness(),
) -> root.ai.Live throws baml.errors.UnknownError {
  let request = s37_Fix_request(task, provider = provider);
  let guarded = s37_guard_channel(provider, channel);
  root.ai.open_live<s37_Patch>(request, guarded)
}

// A full driver sends the negotiated control events as data, then opens the
// the same hand-written request expansion on that guarded channel.
function s37_Fix_drive_manual(
  task: string,
  channel: root.ai.Channel,
  controls: s37_OutEvent[],
  provider: root.ai.Provider = s37_claude_harness(),
) -> root.ai.Live throws baml.errors.UnknownError {
  let guarded = s37_guard_channel(provider, channel);
  for (let control in controls) {
    let landed = s37_try_send(guarded, control);
    if (!landed) { log.warn("control verb rejected: " + s37_verb_name(control)); }
  }
  root.ai.open_live<s37_Patch>(s37_Fix_request(task, provider = provider), guarded)
}

// Tools are supplied at call time. They are not a task declaration field in
// this reference, matching the current BEPv2 exercise constraint.
function s37_Fix_agent_request(
  task: string,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
  provider: root.ai.Provider = s37_claude_harness(),
) -> root.ai.Request<s37_Patch> throws never {
  root.ai.task_request<s37_Patch>(
    provider,
    "s37_Fix",
    "ns_ai_scenarios",
    { "task": task },
    tools,
    root.ai.RequestOptions { dispatch: dispatch },
    {},
    bepv2_prompt_template("Fix the bug: " + task, true),
  )
}

function s37_Fix_agent_manual(
  task: string,
  tools: root.ai.Tool[],
  dispatch: (root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never,
  provider: root.ai.Provider = s37_claude_harness(),
  budget: root.ai.Budget? = null,
) -> root.ai.Done<s37_Patch> | root.ai.BudgetReached | root.ai.Handoff
    throws baml.errors.ToolError | baml.errors.UnknownError {
  root.ai.run_agent<s37_Patch>(
    s37_Fix_agent_request(task, tools, dispatch, provider = provider),
    budget = budget,
  )
}

function s37_edit_tool() -> root.ai.Tool throws never {
  root.ai.Tool.typed<s37_EditArgs>("edit", "Edit a file")
}

function s37_edit_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  let results: root.ai.ToolResult[] = [];
  for (let call in calls) {
    let result = root.ai.invoke_tool<s37_EditArgs>(call, (args: s37_EditArgs) -> string {
      "edited " + args.path
    });
    results.push(result);
  }
  results
}

function s37_collect_live_text(live: root.ai.Live) -> string throws never {
  let text = "";
  for (let event in live.events()) {
    match (event) {
      let delta: root.ai.TranscriptDelta => { text = text + delta.text; },
      _ => null,
    }
  }
  text
}

function s37_drivable(provider: root.ai.Provider) -> string throws never {
  match (provider) {
    let realtime: root.ai.Realtime => "drivable harness",
    _ => "not a harness",
  }
}

function s37_supports(provider: root.ai.Provider, verb: string) -> bool throws never {
  match (provider) {
    let control: root.ai.HarnessControlPlane => control.supported_verbs().includes(verb),
    _ => false,
  }
}

test "37: a harness is Realtime and Tools but does not fake Generate" {
  let harness: root.ai.Provider = s37_claude_harness();
  assert.equal(s37_drivable(harness), "drivable harness")
  assert.is_true(harness is root.ai.Tools)
  assert.is_true(!(harness is root.ai.Generate))
}

test "37: bound config describes sandbox, permission mode, and tool posture" {
  let harness = s37_claude_harness();
  assert.equal(harness.model ?? "", "claude-sonnet-4-6")
  assert.equal(harness.sandbox, s37_Sandbox.Container)
  assert.equal(harness.permission_mode, s37_PermissionMode.AcceptEdits)
  assert.equal(harness.allowed_tools.length(), 3)
}

test "37: divergent harnesses advertise different control verbs" {
  let claude: root.ai.Provider = s37_claude_harness();
  let pi: root.ai.Provider = s37_pi_harness();
  assert.is_true(s37_supports(claude, "RewindFiles"))
  assert.is_true(!s37_supports(pi, "RewindFiles"))
  assert.is_true(s37_supports(pi, "Steer"))
}

test "37: the guard rejects a Pi rewind before it reaches the channel" {
  let recording = s37_RecordingChannel { frames: [] };
  let guarded = s37_guard_channel(s37_pi_harness(), recording);
  assert.is_true(!s37_try_send(guarded, s37_RewindFiles { to_user_message_id: "msg_1", dry_run: false }))
  assert.is_true(s37_try_send(guarded, s37_Steer { text: "use the existing logger" }))
  assert.is_true(recording.frames.join("|").includes("Steer:"))
  assert.is_true(!recording.frames.join("|").includes("RewindFiles:"))
}

test "37: the handwritten live companion drives a fresh harness session" {
  let recording = s37_RecordingChannel { frames: [] };
  let live = s37_Fix_live_manual("auth.py null dereference", recording, provider = s37_claude_harness());
  assert.equal(s37_collect_live_text(live), "patched null dereference")
  assert.is_true(recording.frames.join("|").includes("TurnDone:msg_1"))
  live.cleanup();
}

test "37: the full driver lands Claude rewind but rejects the same Pi control" {
  let claude_frames = s37_RecordingChannel { frames: [] };
  let claude_live = s37_Fix_drive_manual(
    "fix auth.py",
    claude_frames,
    [
      s37_Steer { text: "use the existing logger" },
      s37_RewindFiles { to_user_message_id: "msg_1", dry_run: true },
    ],
    provider = s37_claude_harness(),
  );
  assert.is_true(claude_frames.frames.join("|").includes("RewindFiles:msg_1"))
  claude_live.cleanup();

  let pi_frames = s37_RecordingChannel { frames: [] };
  let pi_live = s37_Fix_drive_manual(
    "fix auth.py",
    pi_frames,
    [
      s37_Steer { text: "keep it small" },
      s37_RewindFiles { to_user_message_id: "msg_1", dry_run: true },
    ],
    provider = s37_pi_harness(),
  );
  assert.is_true(pi_frames.frames.join("|").includes("Steer:keep it small"))
  assert.is_true(!pi_frames.frames.join("|").includes("RewindFiles:msg_1"))
  pi_live.cleanup();
}

test "37: host-owned dispatch runs through the handwritten agent companion" {
  let outcome = s37_Fix_agent_manual(
    "fix auth.py",
    [s37_edit_tool()],
    s37_edit_dispatch,
    provider = s37_claude_harness(),
  );
  match (outcome) {
    let done: root.ai.Done<s37_Patch> => {
      assert.equal(done.value.summary, "fixed")
      assert.equal(done.value.files_changed[0], "auth.py")
    },
    _ => assert.is_true(false),
  }
}

test "37: a transport failure carries replay facts, not a retry opinion" {
  let failure: root.ai.Failure = s37_HarnessSpawnFailure { message: "binary not found" };
  assert.equal(failure.kind(), root.ai.FailureKind.Transport)
  assert.equal(failure.commit_state(), root.ai.CommitState.NotCommitted)
}

testset "integ-test-37-harness-basics" {
  test "37 live: the same hand-written request expansion works with a real Realtime provider" {
    let channel = s37_RecordingChannel { frames: [] };
    let live = s37_Fix_live_manual(
      "Reply with a one-sentence patch plan for a null dereference.",
      channel,
      provider = root.ai.OpenAiRealtime {
        model: "gpt-realtime",
        voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      },
    );
    assert.is_true(s37_collect_live_text(live).length() > 0)
    live.cleanup();
  }
}
