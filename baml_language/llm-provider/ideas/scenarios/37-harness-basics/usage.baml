// ============================================================================
// 37-harness-basics / usage.baml   (APPLICATION-author code)
//
// What an app author writes to use a harness. The whole point: a harness is a
// `Provider`, so the `client → function ... -> Provider` sugar covers it with no
// special case. `sandbox`/`permission_mode`/`model` are bound fields; the live
// control channel is handed in at `.live`.
// ============================================================================

// ---- clients are config-only (the live channel is NOT a client field) -------

client CodeAgent() {
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: Sandbox.Container,
    permission_mode: PermissionMode.AcceptEdits,
    allowed_tools: ["Read", "Edit", "Bash", "Grep", "Glob"],
    disallowed_tools: ["Bash(rm*)"],
    cwd: "/workspace",
  }
}

// Second harness, divergent shape (no rewind, no interrupt). Same client sugar.
client PiCoder() {
  PiAgent { model: "anthropic/claude-sonnet-4-6", cwd: "/workspace" }
}

// Dynamic selection — a plain function body, impossible with a static block.
client Coder(trust: string) {
  if (trust == "ci") {
    ClaudeCode {
      model: "claude-sonnet-4-6",
      sandbox: Sandbox.Virtual,
      permission_mode: PermissionMode.BypassPermissions,   // no prompts in CI
      allowed_tools: null,                                 // all tools
      disallowed_tools: null,
      cwd: "/workspace",
    }
  } else {
    ClaudeCode.in_dir("/workspace")                        // safe defaults via factory
  }
}

// A harness made resilient by a combinator chain reads exactly like a chat
// client: fall back to Pi if Claude Code can't start, cap startup retries.
// (Fallback/Retry forward Realtime by runtime delegation -- proposal §6.)
client ResilientCoder() {
  CodeAgent().fallback_to(PiCoder()).with_retry(2)
}

// ---- the ONE-SHOT entry point: a harness has NO `.call` ----------------------
// A harness drops the degenerate `call` (provider-as-marker.md §2), so the plain
// `Fix(task) -> Patch` value-function does NOT resolve against `CodeAgent()`: its
// companion `match`es `HttpProvider` (and, via the call<->stream degrade, a
// `Streaming` provider) and a harness is neither. A "fresh session, run to
// completion" one-shot is therefore expressed as a `Realtime.run` over a
// throwaway channel that just collects the final transcript — i.e. it is `.live`
// with a do-nothing driver, not a separate `call`. (This is Python `query()`,
// which under the hood is also a session run to completion.)

class Patch {
  files_changed: string[]
  summary: string
}

// One-shot = drive a session that we never steer, then read the transcript.
function Fix(task: string) -> HarnessTranscript {
  client: CodeAgent()
  prompt #"Fix the bug: {{ task }}"#
}
// Fix("auth.py null deref") desugars to the `.live` companion with an empty
// driver: match (CodeAgent()) { let r: Realtime => r.run(prompt, fresh_channel()), ... }
// There is no `ClaudeCode.call<Patch>(prompt)` to call — the harness has no call.

// ---- the STATEFUL, driven session: the `.live` companion ---------------------
// `.live(args, io)` is the Realtime companion. The app hands in a Channel and
// then DRIVES the session by `send`ing OutEvents and reacting to InEvents. The
// function below is what the proposal's `.live` desugars to; we write it
// longhand to show the negotiation + guard wiring an app author actually does.

function Build(spec: string, io: Channel) -> HarnessTranscript {
  client: CodeAgent()
  prompt #"Implement this spec, then run the tests:\n{{ spec }}"#
}
// Desugars (proposal §5) to:
//   let p = CodeAgent();
//   match (p) {
//     let r: Realtime => r.run(render_prompt(spec), io),
//     _               => throw baml.errors.Unsupported { message: "client is not realtime" },
//   }

// ---- a full driver: negotiate the verb set, then steer/interrupt/rewind ------
// This is the app author's OWN code (not generated). It shows the control plane
// as messages over the handed-in channel, plus capability negotiation so the UI
// only offers verbs the chosen harness actually backs.

function DriveBuild(spec: string, raw_io: Channel) -> HarnessTranscript
    throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
  let p = CodeAgent();

  // The harness must be Realtime to be driven; a one-shot can't fake a session.
  match (p) {
    let r: Realtime => {
      // Negotiate: wrap the host channel so unsupported verbs are rejected
      // locally (Claude Code in a Container honors RewindFiles; Pi would not).
      let io = GuardedChannel.negotiate(raw_io, r);

      // Wire up the event stream coming UP from the session.
      io.on(ev => {
        match (ev) {
          let d: TextDelta     => { baml.io.print(d.text); },
          let ask: PermissionAsk => {
            // human-in-the-loop: a destructive Bash gets denied, edits allowed.
            // (We answer by sending a follow-up control message in a real UI;
            //  shown abbreviated here.)
            baml.io.print("permission requested: " + ask.name);
          },
          let done: TurnDone   => { baml.io.print("[turn done] checkpoint=" + done.user_message_id); },
          let rr: RewindResult => { baml.io.print("[reverted] " + rr.reverted_paths.length().to_string() + " files"); },
          let e: SessionError  => { baml.io.print("[error] " + e.message); },
          _                    => {},
        }
      });

      // Drive it. Each of these is a MESSAGE, not a method call on the client.
      io.send(Steer { text: "Use the existing logger, don't add a new one." });   // immediate
      io.send(FollowUp { text: "When green, open a PR." });                        // queued
      io.send(SetPermMode { mode: PermissionMode.Default });                       // tighten mid-run

      // Suppose the model goes down a wrong path: interrupt + rewind to the last
      // good checkpoint. `GuardedChannel` lets these through ONLY because this
      // client is a containerized ClaudeCode; the same code against `PiCoder()`
      // would raise baml.errors.Io at the guard before touching the subprocess.
      io.send(Interrupt {});
      io.send(RewindFiles { to_user_message_id: "msg_3", dry_run: false });

      io.send(EndSession {});

      // `run` blocks driving the duplex until EndSession settles, then returns
      // the opaque transcript.
      r.run(render_prompt(spec), io)
    },
    _ => throw baml.errors.Unsupported { message: "CodeAgent is not a steerable harness" },
  }
}

// ---- host-owned tool dispatch (optional power-user path) ---------------------
// If the app wants its OWN dispatch policy (budget, approval, parallelism) it
// uses the Tools companion instead of letting the harness run tools internally.
// Same provider, different capability — `.run_tools` vs `.live`.

function BuildWithMyTools(spec: string, my_tools: Tool[], ctx: ExecutionContext) -> Patch
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let p = CodeAgent();
  match (p) {
    let t: Tools => t.run_tools<Patch>(render_prompt(spec), my_tools, ctx),
    _            => throw baml.errors.Unsupported { message: "client cannot host-dispatch tools" },
  }
}
