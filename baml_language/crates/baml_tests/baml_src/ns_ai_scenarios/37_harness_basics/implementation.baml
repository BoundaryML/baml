// Scenario 37 — a coding harness is a marker provider whose honest
// interactions are Realtime and Tools. It does not fake Generate.

enum s37_Sandbox {
  Local
  Container
  Virtual
}

enum s37_PermissionMode {
  Default
  AcceptEdits
  Plan
  BypassPermissions
}

// Driver -> harness control messages are data over one Channel. Adding a verb
// extends this closed vocabulary without adding another capability method.
class s37_Steer { text: string }
class s37_FollowUp { text: string }
class s37_Interrupt {}
class s37_SetModel { model: string? }
class s37_SetPermissionMode { mode: s37_PermissionMode }
class s37_RewindFiles { to_user_message_id: string, dry_run: bool }
class s37_StopTask { task_id: string }
class s37_EndSession {}

type s37_OutEvent =
    s37_Steer
  | s37_FollowUp
  | s37_Interrupt
  | s37_SetModel
  | s37_SetPermissionMode
  | s37_RewindFiles
  | s37_StopTask
  | s37_EndSession

function s37_verb_name(event: s37_OutEvent) -> string throws never {
  match (event) {
    let value: s37_Steer => "Steer",
    let value: s37_FollowUp => "FollowUp",
    let value: s37_Interrupt => "Interrupt",
    let value: s37_SetModel => "SetModel",
    let value: s37_SetPermissionMode => "SetPermissionMode",
    let value: s37_RewindFiles => "RewindFiles",
    let value: s37_StopTask => "StopTask",
    let value: s37_EndSession => "EndSession",
  }
}

function s37_permission_mode_name(mode: s37_PermissionMode) -> string throws never {
  match (mode) {
    s37_PermissionMode.Default => "default",
    s37_PermissionMode.AcceptEdits => "accept-edits",
    s37_PermissionMode.Plan => "plan",
    s37_PermissionMode.BypassPermissions => "bypass-permissions",
  }
}

function s37_encode(event: s37_OutEvent) -> string throws never {
  match (event) {
    let value: s37_Steer => "Steer:" + value.text,
    let value: s37_FollowUp => "FollowUp:" + value.text,
    let value: s37_Interrupt => "Interrupt:",
    let value: s37_SetModel => "SetModel:" + (value.model ?? ""),
    let value: s37_SetPermissionMode => "SetPermissionMode:" + s37_permission_mode_name(value.mode),
    let value: s37_RewindFiles => "RewindFiles:" + value.to_user_message_id,
    let value: s37_StopTask => "StopTask:" + value.task_id,
    let value: s37_EndSession => "EndSession:",
  }
}

function s37_all_verbs() -> string[] throws never {
  ["Steer", "FollowUp", "Interrupt", "SetModel", "SetPermissionMode", "RewindFiles", "StopTask", "EndSession"]
}

// Control-plane negotiation uses the shared HarnessControlPlane capability.
// String verb names remain scenario-local because Channel transports strings.

class s37_HarnessSpawnFailure {
  message: string,

  implements root.ai.Failure {
    function kind(self) -> root.ai.FailureKind throws never { root.ai.FailureKind.Transport }
    function commit_state(self) -> root.ai.CommitState throws never { root.ai.CommitState.NotCommitted }
    function retry_after(self) -> baml.time.Duration? throws never { null }
    function is_resumable(self) -> bool throws never { false }
  }
}

class s37_RecordingChannel {
  frames: string[],

  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io {
      self.frames.push(data);
      null
    }
    function close(self) -> null throws never { null }
  }
}

class s37_NullChannel {
  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never { null }
    function send(self, data: string) -> null throws baml.errors.Io { null }
    function close(self) -> null throws never { null }
  }
}

// The driver-side guard rejects unsupported control verbs before they reach
// the subprocess transport.
class s37_GuardedChannel {
  inner: root.ai.Channel,
  allowed: string[],

  implements root.ai.Channel {
    function on(self, handler: (string) -> null throws never) -> null throws never {
      self.inner.on(handler)
    }
    function send(self, data: string) -> null throws baml.errors.Io {
      let verb = "";
      for (let candidate in s37_all_verbs()) {
        if (data.starts_with(candidate + ":")) { verb = candidate; }
      }
      if (verb != "" && !self.allowed.includes(verb)) {
        throw baml.errors.Io { message: "harness does not support control verb: " + verb };
      }
      self.inner.send(data)
    }
    function close(self) -> null throws never { self.inner.close() }
  }
}

function s37_guard_channel(provider: root.ai.Provider, channel: root.ai.Channel) -> root.ai.Channel throws never {
  match (provider) {
    let control: root.ai.HarnessControlPlane => s37_GuardedChannel {
      inner: channel,
      allowed: control.supported_verbs(),
    },
    _ => channel,
  }
}

function s37_try_send(channel: root.ai.Channel, event: s37_OutEvent) -> bool throws never {
  channel.send(s37_encode(event)) catch (e) {
    _ => { return false; },
  };
  true
}

class s37_CompletedLive {
  text: string,
  channel: root.ai.Channel,

  implements root.ai.Live {
    function events(self) -> root.ai.LiveEventStream throws never {
      [root.ai.TranscriptDelta { text: self.text }, root.ai.LiveClosed {}]
    }
    function cancel_response(self) -> null throws baml.errors.UnknownError { null }
    function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError { null }
    function close(self) -> null throws never { self.channel.close() }
    function cleanup(self) -> null throws never { self.close() }
  }
}

class s37_HarnessState {
  remaining: root.ai.ToolCall[],
  final_json: string,
}

class s37_ScriptedHarness {
  name: string,
  model: string?,
  sandbox: s37_Sandbox,
  permission_mode: s37_PermissionMode,
  allowed_tools: string[],
  verbs: string[],
  reply: string,
  tool_calls: root.ai.ToolCall[],
  final_json: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { self.name }
    // Used only to render PromptAst in the hand-written request. A subprocess
    // harness still has no Generate implementation.
    function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  }

  implements root.ai.Realtime {
    function open_live<T>(
      self,
      request: root.ai.Request<T>,
      channel: root.ai.Channel,
    ) -> root.ai.Live throws baml.errors.UnknownError {
      channel.send("TurnStart:") catch (e) { _ => null };
      channel.send("TextDelta:" + self.reply) catch (e) { _ => null };
      channel.send("TurnDone:msg_1") catch (e) { _ => null };
      s37_CompletedLive { text: self.reply, channel: channel }
    }
  }

  implements root.ai.HarnessControlPlane {
    function supported_verbs(self) -> string[] throws never { self.verbs }
  }

  implements root.ai.Tools {
    type Transcript = s37_HarnessState

    function begin<T>(self, request: root.ai.Request<T>) -> s37_HarnessState
        throws baml.errors.ToolError | baml.errors.UnknownError {
      s37_HarnessState { remaining: self.tool_calls, final_json: self.final_json }
    }
    function step<T>(self, transcript: s37_HarnessState) -> T | root.ai.ToolCalls
        throws baml.errors.ToolError | baml.errors.UnknownError {
      match (transcript.remaining.at(0)) {
        let call: root.ai.ToolCall => root.ai.ToolCalls { calls: [call] },
        null => baml.json.from_string<T>(transcript.final_json) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["scripted harness final parse failed"] },
        },
      }
    }
    function submit(
      self,
      transcript: s37_HarnessState,
      results: root.ai.ToolResult[],
    ) -> s37_HarnessState throws baml.errors.ToolError | baml.errors.UnknownError {
      s37_HarnessState {
        remaining: transcript.remaining.slice(1, transcript.remaining.length()),
        final_json: transcript.final_json,
      }
    }
  }
}

function s37_claude_harness() -> s37_ScriptedHarness throws never {
  s37_ScriptedHarness {
    name: "scripted-claude-code",
    model: "claude-sonnet-4-6",
    sandbox: s37_Sandbox.Container,
    permission_mode: s37_PermissionMode.AcceptEdits,
    allowed_tools: ["read", "edit", "bash"],
    verbs: s37_all_verbs(),
    reply: "patched null dereference",
    tool_calls: [root.ai.ToolCall { id: "edit-1", name: "edit", args: `{"path":"auth.py"}` }],
    final_json: `{"files_changed":["auth.py"],"summary":"fixed"}`,
  }
}

function s37_pi_harness() -> s37_ScriptedHarness throws never {
  s37_ScriptedHarness {
    name: "scripted-pi",
    model: "anthropic/claude-sonnet-4-6",
    sandbox: s37_Sandbox.Local,
    permission_mode: s37_PermissionMode.Default,
    allowed_tools: ["read", "edit"],
    verbs: ["Steer", "FollowUp", "EndSession"],
    reply: "pi completed the task",
    tool_calls: [],
    final_json: `{"files_changed":[],"summary":"pi done"}`,
  }
}
