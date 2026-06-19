// ============================================================================
// 37-harness-basics / implement.baml   (std-lib / library-author code)
//
// A harness (Claude Code, Pi, Flue, ...) is the payoff of the base-`Provider`
// model: its transport is a *subprocess*, not `baml.http`, so it is NOT an
// `HttpProvider`. It implements:
//   - Provider   (one-shot `call<T>`: spawn -> run to completion -> typed answer)
//   - Realtime   (the long-lived steerable session; control plane = messages
//                 over the handed-in Channel)
//   - Tools      (so the host can also drive its own tool dispatch policy)
//
// The control plane (interrupt / setModel / setPermissionMode / rewindFiles /
// steer / followUp / stopTask) is NOT a pile of methods on the client. It is a
// closed set of OutEvents the caller `send`s over the `io: Channel` that
// `Realtime.run` is handed, and the harness replies with InEvents. The harness
// is *driven*, not *called*.
//
// `sandbox` / `permission_mode` / `model` are bound config FIELDS (like `model`
// on a chat client). The live channel is a *parameter* of `Realtime.run`.
// ============================================================================

// ----------------------------------------------------------------------------
// (1) Bound config: enums and the sandbox descriptor. NET-NEW host types.
// ----------------------------------------------------------------------------

enum Sandbox {
  Local            // run shell commands directly on this machine
  Container        // run inside an ephemeral container, fs scoped to a workdir
  Virtual          // a fully virtualized fs+net, nothing escapes the box
}

enum PermissionMode {
  Default          // ask on risky actions
  AcceptEdits      // auto-approve file edits, ask on the rest
  Plan             // read & propose, never act
  BypassPermissions// no prompts (trusted CI only)
}

// ----------------------------------------------------------------------------
// (2) The control-plane vocabulary. NET-NEW: the harness's InEvent / OutEvent
//     instantiations of the Channel seam. `Channel` itself is canonical
//     (_conventions.md); these are the *concrete* event unions a harness speaks.
//
// OutEvent = things the DRIVER sends down to the running session.
// InEvent  = things the session emits up to the driver.
//
// This is the whole control plane. There is no `setModel()` method; there is a
// `SetModel` OutEvent. That is the load-bearing move: a steerable session has an
// open-ended, evolving verb set, and modeling it as data over one duplex method
// keeps `Realtime` from growing a method per harness feature.
// ----------------------------------------------------------------------------

// --- driver -> session (the control plane) ---
class Steer       { text: string }                     // immediate mid-turn injection
class FollowUp    { text: string }                     // queued: runs after this turn
class Interrupt   {}                                   // stop the in-flight turn at a safe boundary
class SetModel    { model: string? }                   // hot-swap model, keep session
class SetPermMode { mode: PermissionMode }             // tighten/loosen mid-run
class RewindFiles { to_user_message_id: string, dry_run: bool }   // undo file edits back to a turn
class StopTask    { task_id: string }                  // kill a background sub-task
class EndSession  {}                                   // graceful shutdown

type OutEvent =
    Steer
  | FollowUp
  | Interrupt
  | SetModel
  | SetPermMode
  | RewindFiles
  | StopTask
  | EndSession

// --- session -> driver (the event stream) ---
class TextDelta     { text: string }                              // assistant text streaming out
class ToolStarted   { task_id: string, name: string, args: map<string, unknown> }
class ToolFinished  { task_id: string, name: string, output: unknown }
class PermissionAsk { ask_id: string, name: string, args: map<string, unknown> }  // human-in-the-loop gate
class TurnDone      { user_message_id: string }                  // a turn settled; checkpoint id for rewind
class RewindResult  { reverted_paths: string[], dry_run: bool }  // reply to a RewindFiles
class SessionError  { message: string }

type InEvent =
    TextDelta
  | ToolStarted
  | ToolFinished
  | PermissionAsk
  | TurnDone
  | RewindResult
  | SessionError

// The opaque record the session settles into when it ends. Provider-owned.
class HarnessTranscript {
  user_message_ids: string[]      // every turn checkpoint, newest last
  final_text: string
  _raw: $rust_type                // the full subprocess event log, opaque
}

// ----------------------------------------------------------------------------
// (3) NET-NEW capability: ControlPlane.
//
// `Realtime.run(prompt, io) -> Transcript` (canonical) already gives us the
// driven duplex session. But a *plain* `Realtime` provider only promises "I will
// drive this channel"; it says nothing about WHICH OutEvents it honors. A voice
// model and a coding harness are both `Realtime`, yet `RewindFiles` is nonsense
// for voice.
//
// `ControlPlane` is a refinement that lets a harness advertise its supported
// verbs at runtime, so a UI can grey out the buttons it can't back. It is
// deliberately a *capability*, not fields on the client — the verb set is a
// property of the running provider, negotiated like every other capability.
// ----------------------------------------------------------------------------

interface ControlPlane requires Realtime {
  // Which OutEvent variants this provider actually honors. A driver consults
  // this before wiring up its buttons. `reflect.type_of` gives first-class
  // `type` values so a verb set is just a list of types.
  function supported_verbs(self) -> type[]
}

// ----------------------------------------------------------------------------
// (4) The harness provider: ClaudeCode.
//
// Transport is a subprocess. NO `implements HttpProvider`. `call` and `run` are
// host-backed (`$rust_io_function`) — the genuine bridge boundary (Open Q7).
// ----------------------------------------------------------------------------

class ClaudeCode {
  model: string?
  sandbox: Sandbox
  permission_mode: PermissionMode
  allowed_tools: string[]?
  disallowed_tools: string[]?
  cwd: string?

  // Static factory — the common "code in this dir" shape (mirrors from_model).
  function in_dir(dir: string) -> ClaudeCode {
    ClaudeCode {
      model: null,
      sandbox: Sandbox.Container,
      permission_mode: PermissionMode.AcceptEdits,
      allowed_tools: ["Read", "Edit", "Bash", "Grep", "Glob"],
      disallowed_tools: null,
      cwd: dir,
    }
  }

  // -- base Provider: the ONE-SHOT entry point (Python SDK `query()`). --------
  // Spawn the subprocess, run the task to completion, parse the final answer
  // into T. No channel: a fresh session, no control plane, no history.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // Drives `claude --print` (or equiv) in `self.sandbox`, with the bound
      // permission_mode / allowed_tools applied as spawn flags, then SAP-parses
      // the final assistant message into T. Subprocess, not HTTP.
      $rust_io_function
    }
  }

  // -- Realtime: the STATEFUL, driven session (TS `query()` -> Query). --------
  // The caller hands in `io`. The harness:
  //   * spawns the subprocess once,
  //   * pumps session events UP as InEvents (io.send),
  //   * lands driver OutEvents (io.on handler) at safe boundaries,
  //   * returns the opaque transcript when the session ends.
  // The whole control plane is here, as message handling — NOT as methods.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript throws baml.errors.LlmClient {
      // Host body owns: subprocess lifecycle, the mid-flight-mutation race
      // ("land Steer/Interrupt/SetModel at a boundary between tool calls or
      // model round-trips without corrupting the in-progress transcript"
      // -- background §2 "What's hard"), and translating each OutEvent variant
      // into the subprocess's stdin protocol:
      //   Steer       -> inject into the current turn's input stream
      //   FollowUp    -> enqueue for after TurnDone
      //   Interrupt   -> SIGINT-at-boundary
      //   SetModel    -> control message; session + history preserved
      //   SetPermMode -> control message
      //   RewindFiles -> checkpoint restore; emits RewindResult back up
      //   StopTask    -> kill the named background task
      //   EndSession  -> drain, settle, return.
      // And UP: TextDelta / ToolStarted / ToolFinished / PermissionAsk /
      // TurnDone / RewindResult / SessionError via io.send.
      $rust_io_function
    }
  }

  // -- ControlPlane: advertise the verbs THIS harness honors. -----------------
  implements ControlPlane {
    function supported_verbs(self) -> type[] {
      // Claude Code honors the full set EXCEPT it only honors RewindFiles when
      // the sandbox can checkpoint the fs — Local can't reliably revert.
      let base = [
        reflect.type_of<Steer>(),
        reflect.type_of<FollowUp>(),
        reflect.type_of<Interrupt>(),
        reflect.type_of<SetModel>(),
        reflect.type_of<SetPermMode>(),
        reflect.type_of<StopTask>(),
        reflect.type_of<EndSession>(),
      ];
      match (self.sandbox) {
        Sandbox.Local => base,                                    // no rewind
        _             => base.append(reflect.type_of<RewindFiles>()),
      }
    }
  }

  // -- Tools: let the HOST own dispatch policy (parallel/approval/budget). -----
  // Canonical `Tools` (begin/step/submit/run_tools). For a harness this routes
  // through the SAME subprocess but surfaces each tool call to the host's
  // ExecutionContext.dispatch instead of the harness's built-in tool runner.
  // The opaque Transcript here is the subprocess loop state.
  implements Tools {
    type Transcript = HarnessTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> HarnessTranscript throws baml.errors.LlmClient {
      // Spawn with host tools registered as external tools the subprocess will
      // call out for (rather than running Bash/Edit itself).
      $rust_io_function
    }

    function step<T>(self, t: HarnessTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      // Advance the subprocess until it either emits a final answer (parse->T)
      // or requests host tools. ToolCall.id is the subprocess's correlation id,
      // OPAQUE to the loop (same contract as the HTTP providers).
      $rust_io_function
    }

    function submit(self, t: HarnessTranscript, results: ToolResult[]) -> HarnessTranscript throws baml.errors.LlmClient {
      // Feed tool outputs back over the subprocess stdin, correlated by id.
      $rust_io_function
    }
    // run_tools<T> inherited from Tools default.
  }
}

// ----------------------------------------------------------------------------
// (5) A second harness with a DIFFERENT shape: Pi.
//
// Pi (background §2) is the cleanest example of the steer-vs-followUp split, and
// it has NO rewind at all. Modeling it on the SAME interfaces proves the
// control-plane-as-events seam is portable: Pi just honors a SMALLER OutEvent
// set. Same `Realtime`/`ControlPlane`, narrower `supported_verbs`.
// ----------------------------------------------------------------------------

class PiAgent {
  model: string
  cwd: string?

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function   // Pi one-shot
    }
  }

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript throws baml.errors.LlmClient {
      // Pi's session.prompt/steer/followUp/subscribe, mapped onto the same
      // OutEvent/InEvent vocabulary. Pi has NO interrupt and NO rewind, so:
      //   - it IGNORES Interrupt / RewindFiles / SetModel / StopTask, OR
      //   - the driver should have consulted supported_verbs and never sent them.
      // Sending an unsupported verb is a no-op + a SessionError up the channel.
      $rust_io_function
    }
  }

  implements ControlPlane {
    function supported_verbs(self) -> type[] {
      // Pi: steer (immediate) + followUp (queued) + end. That's it.
      [
        reflect.type_of<Steer>(),
        reflect.type_of<FollowUp>(),
        reflect.type_of<EndSession>(),
      ]
    }
  }
}

// ----------------------------------------------------------------------------
// (6) A driver combinator: the canonical `.live` companion errors if the
//     provider isn't Realtime. But a harness driver also wants to (a) negotiate
//     the verb set and (b) reject sending an unsupported verb BEFORE it reaches
//     the subprocess. `GuardedChannel` wraps the host's Channel and drops/raises
//     on verbs the provider didn't advertise. Pure library code — no host body.
// ----------------------------------------------------------------------------

class GuardedChannel {
  inner: Channel
  allowed: type[]        // from provider.supported_verbs()

  implements Channel {
    function on(self, handler: (InEvent) -> void) -> null {
      self.inner.on(handler)
    }

    function send(self, data: OutEvent) -> null throws baml.errors.Io {
      // Reflect the runtime type of the OutEvent and check it against the
      // negotiated set. This is the SAME interface-membership reflection the
      // capability `match` uses, applied to a closed data union.
      let ty = reflect.type_of_value(data);
      for (let allowed in self.allowed) {
        if (ty == allowed) { return self.inner.send(data); }
      }
      throw baml.errors.Io { message: "harness does not support this control verb" }
    }

    function close(self) -> null { self.inner.close() }
  }

  // Build a guarded channel by negotiating with whatever Realtime provider this
  // is. If it's also ControlPlane, honor its advertised verbs; else allow all
  // (best-effort) and let the session itself reject.
  function negotiate(io: Channel, p: Realtime) -> Channel {
    match (p) {
      let cp: ControlPlane => GuardedChannel { inner: io, allowed: cp.supported_verbs() },
      _                    => io,    // no advertised verb set: pass through raw
    }
  }
}
