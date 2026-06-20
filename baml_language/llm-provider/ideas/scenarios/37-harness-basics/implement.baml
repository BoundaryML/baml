// ============================================================================
// 37-harness-basics / implement.baml   (std-lib / library-author code)
//
// A harness (Claude Code, Pi, Flue, ...) is the payoff of the provider-as-MARKER
// model: its transport is a *subprocess*, not `baml.http`, so it is NOT an
// `HttpProvider` and it has NO `call` (provider-as-marker.md §2 — a harness drops
// the old degenerate "spawn-and-hope `call`"). It is a `Provider` only in the
// marker sense (it is composable, the type `client` returns); what it can DO is
// per-capability. It implements:
//   - Realtime   (the long-lived steerable session; control plane = messages
//                 over the handed-in Channel) — this is its REAL interaction, and
//                 it is also how you get a one-shot: `run` over a throwaway channel
//   - Tools      (so the host can also drive its own tool dispatch policy)
// All three of these `requires Provider`, so `ClaudeCode` IS a `Provider`
// automatically with no explicit `implements Provider` block.
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
// (2b) Concrete harness errors. A subprocess harness has its OWN domain failures
//      that are neither the answer nor a foreign transport error, so they get
//      concrete typed classes (error-model rule 2). Each `implements` whichever
//      capability interfaces it can actually arise on. Since a harness has NO
//      `call` (provider-as-marker.md), there is NO CallError channel here: a
//      spawn failure surfaces on the driven session (RealtimeError) and the
//      host-dispatch tool loop (ToolError), the harness's only two interactions.
//      None are rate-limit / parse failures, so those classifiers are false;
//      spawn failures ARE classified as network-ish (the subprocess bridge is
//      the harness's "transport"), so `is_network_error` is true there.
class HarnessSpawnError {
  message: string                 // e.g. "claude binary not found", "sandbox denied"
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return true; }   // subprocess bridge = transport
  }
  implements baml.errors.ToolError {
    function is_network_error(self) -> bool { return true; }
  }
}

// The harness ran but the model declined the task (HTTP-200-equivalent refusal
// surfaced in the subprocess event log). Canonical `Refused` shape, on the
// realtime/tool channels because a harness declines mid-session or mid-tool-loop
// (no `call`, so no CallError variant here).
class Refused {
  category: string
  message: string
  implements baml.errors.RealtimeError {
    function is_network_error(self) -> bool { return false; }
  }
  implements baml.errors.ToolError {
    function is_network_error(self) -> bool { return false; }
  }
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
// Transport is a subprocess. NO `implements HttpProvider`, NO `call`. `run` and
// the `Tools` methods are host-backed (`$rust_io_function`) — the genuine bridge
// boundary (Open Q7).
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

  // -- No `implements Provider { call }`. ------------------------------------
  // A harness drives a subprocess; it does NOT do HTTP request/response, so it
  // does not own `call` (provider-as-marker.md §2: harnesses drop the degenerate
  // best-effort `call`). `ClaudeCode` IS a `Provider` (the marker) automatically,
  // because `Realtime`/`Tools`/`ControlPlane` all `requires Provider`. Its REAL
  // interactions are below: the driven session (`Realtime.run`) and host-owned
  // tool dispatch (`Tools`). A fresh one-shot ("Python `query()`") is just a
  // `run` over a throwaway channel (or `run_tools`) — there is no separate,
  // honest `call<T>` to fake.

  // -- Realtime: the STATEFUL, driven session (TS `query()` -> Query). --------
  // The caller hands in `io`. The harness:
  //   * spawns the subprocess once,
  //   * pumps session events UP as InEvents (io.send),
  //   * lands driver OutEvents (io.on handler) at safe boundaries,
  //   * returns the opaque transcript when the session ends.
  // The whole control plane is here, as message handling — NOT as methods.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
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
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "claude code session failed");
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
        _             => base.concat([reflect.type_of<RewindFiles>()]),
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

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Spawn with host tools registered as external tools the subprocess will
      // call out for (rather than running Bash/Edit itself).
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code begin failed");
    }

    function step<T>(self, t: HarnessTranscript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Advance the subprocess until it either emits a final answer (parse->T)
      // or requests host tools. ToolCall.id is the subprocess's correlation id,
      // OPAQUE to the loop (same contract as the HTTP providers).
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code step failed");
    }

    function submit(self, t: HarnessTranscript, results: ToolResult[]) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Feed tool outputs back over the subprocess stdin, correlated by id.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "claude code submit failed");
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

  // No `implements Provider { call }` — like ClaudeCode, Pi is a driven
  // subprocess, not an HTTP request/response provider. It IS a `Provider`
  // (marker) via `Realtime`/`ControlPlane` requiring it. Its real interaction is
  // the driven session below.

  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      // Pi's session.prompt/steer/followUp/subscribe, mapped onto the same
      // OutEvent/InEvent vocabulary. Pi has NO interrupt and NO rewind, so:
      //   - it IGNORES Interrupt / RewindFiles / SetModel / StopTask, OR
      //   - the driver should have consulted supported_verbs and never sent them.
      // Sending an unsupported verb is a no-op + a SessionError up the channel.
      $rust_io_function
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.RealtimeError>(e, "pi session failed");
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
