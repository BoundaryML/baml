// ============================================================================
// 42-harness-abstraction / usage.baml   (APPLICATION-author code)
//
// The payoff: an app binds ONE agent interface (HarnessAgent) and drives Claude
// Code, Pi, or Codex underneath — swapping runtimes is a one-line client edit,
// and harness output flows into the SAME .stream/.live surfaces as a plain model
// call, because both are `Provider`s.
//
// Shows:
//   - how LITTLE the app writes for the common cases (one-shot, plain stream)
//   - the runtime swap (ClaudeCodeHarness <-> PiHarness) at one call site
//   - the SERVER-BOUNDARY dance (two stateless handlers, one logical session)
//   - the ESCAPE HATCH where the uniform surface is lossy (rewindFiles on Claude,
//     which Pi can't do) — proving portability AND exposing where it leaks.
// ============================================================================


// ---- clients: config-only; the runtime is the only thing that changes --------

// One agent interface, Claude Code underneath. `HarnessAgent { inner: ... }` is
// the AI-SDK `new HarnessAgent({ harness: claudeCode(...) })`.
client CodeAgent() {
  HarnessAgent {
    inner: ClaudeCodeHarness {
      model: "claude-sonnet-4-6",
      sandbox: Sandbox.Container,
      permission_mode: PermissionMode.AcceptEdits,
      allowed_tools: ["Read", "Edit", "Bash", "Grep", "Glob"],
      cwd: "/workspace",
    }
  }
}

// SAME app code downstream — only the inner runtime differs. This is the whole
// abstraction claim: Pi is a different session model (tree, no rewind, no
// interrupt) and the client block is the ONLY place that knows.
client PiAgentClient() {
  HarnessAgent {
    inner: PiHarness { model: "anthropic/claude-sonnet-4-6", cwd: "/workspace" }
  }
}

// Dynamic runtime selection — a plain function body, impossible with a static
// block. Pick the runtime per tenant; everything downstream is unchanged.
client Coder(runtime: string) {
  if (runtime == "pi") { PiAgentClient() }
  else                 { CodeAgent() }
}

// A harness made resilient reads exactly like a chat client: if Claude Code can't
// lease a sandbox, fall back to Pi. Fallback forwards Realtime/Streaming by
// runtime delegation (proposal §6) — and forwards Harness too, since HarnessAgent
// claims it. (Caveat: Fallback does NOT forward `Harness.attach` to the SAME
// runtime a handle came from — see evaluation (c). For lifecycle, hold a single
// HarnessAgent, not a Fallback.)
client ResilientCoder() {
  CodeAgent().fallback_to(PiAgentClient()).with_retry(2)
}


// ---- the common case: one-shot. A harness call reads like any model call ------

class Patch {
  files_changed: string[]
  summary: string
}

// Identical in shape to `Extract(resume) -> Resume` over OpenAI. The app author
// writes NOTHING harness-specific. `.call` spins a fresh session to completion.
function Summarize(task: string) -> Patch {
  client: CodeAgent()
  prompt #"Inspect the repository and summarize the test setup, then: {{ task }}"#
}
// Summarize("note any flaky tests")  ->  HarnessAgent.call<Patch>(prompt)
//                                        ->  ClaudeCodeHarness.call<Patch>(prompt)


// ---- harness output INTO the same streaming UI as a plain call ---------------
// `.stream` desugars to the §5 companion. Because HarnessAgent implements the
// canonical `Streaming`, this returns the SAME baml.llm.Stream a chat model emits
// — the heavy Claude Code runtime plugs into the exact `toUIMessageStream` surface
// a one-shot `streamText` uses. The app author cannot tell them apart here.

function SummarizeStream(task: string) -> Patch {
  client: CodeAgent()
  prompt #"Inspect the repo and summarize: {{ task }}"#
}
// SummarizeStream.stream("...")  ->  desugars (§5) to:
//   match (p) {
//     let s: Streaming => s.stream<string, Patch>(render_prompt(task)),  // <- HarnessAgent.stream
//     _               => buffer_as_stream(p.call<Patch>(render_prompt(task))),
//   }
// The match takes the Streaming arm: harness text deltas arrive as plain stream
// items, the settled transcript as the final Patch.


// ---- a RICH agent UI: subscribe to the full event log (tool cards, asks) -----
// When the app wants tool-call cards and permission prompts (not just text), it
// uses the AgentStreaming companion to get the lossless InEvent stream. Still ONE
// interface; just a richer projection.

function DriveRich(task: string) -> HarnessTranscript throws baml.errors.LlmClient {
  let p = CodeAgent();
  match (p) {
    let a: AgentStreaming => {
      let evs = a.events(render_prompt(task), null);
      // a real UI renders each variant as a card; abbreviated here.
      // for (let ev in evs) { match (ev) { let t: ToolStarted => render_card(t), ... } }
      drain_to_transcript(evs)
    },
    _ => throw baml.errors.LlmClient { message: "client does not expose an agent event log" },
  }
}


// ============================================================================
// SERVER-BOUNDARY: two stateless handlers, one logical session.
//
// This is the part the proposal's in-process `Realtime.run(io)` cannot express
// alone — the live channel only exists within one request. We use the net-new
// `Harness` lifecycle (create/detach/attach) via the `ServerSession` combinator.
// ============================================================================

// app singletons (built once at boot)
client Server() {
  // ServerSession is NOT a Provider — it is the cross-request driver. It is built
  // from a HarnessAgent + a handle store. (A `client` here is being used loosely
  // to name a configured value; in real code this is a top-level binding.)
  HarnessAgent {
    inner: ClaudeCodeHarness {
      model: "claude-sonnet-4-6",
      sandbox: Sandbox.Container,
      permission_mode: PermissionMode.AcceptEdits,
      allowed_tools: ["Read", "Edit", "Bash"],
      cwd: "/workspace",
    }
  }
}

function build_server() -> ServerSession {
  ServerSession { agent: Server(), service: HandleStore.new() }
}

// POST /agent/start  -> creates the session, runs the first turn, parks it, and
// returns the session id for the client to store in a cookie.
function HandleStart(spec: string, io: Channel) -> string throws baml.errors.LlmClient {
  let srv = build_server();
  let h = srv.open(render_prompt(spec), SessionOpts { cwd: "/workspace", resume_from: null, permission_mode: null }, io);
  return h.id;     // the app stores this; the live sandbox is parked, claimable
}

// POST /agent/:id/continue  -> a DIFFERENT request, possibly a different server
// process. Reattaches the parked session over a NEW channel and runs another turn.
// The `session expired` path is the server-authoritative reality: if the sandbox
// was GC'd while the user was away, no amount of client-side state can recover it.
function HandleContinue(id: string, io: Channel) -> string throws baml.errors.LlmClient {
  let srv = build_server();
  let h = srv.resume(id, io);     // throws "expired; start a new one" if reclaimed
  return h.id;
}

// DELETE /agent/:id  -> tear down the sandbox + child process.
function HandleEnd(id: string) -> null throws baml.errors.LlmClient {
  let srv = build_server();
  srv.close(id)
}


// ============================================================================
// THE ESCAPE HATCH: a feature the uniform surface CANNOT name.
//
// `rewindFiles` is Claude-Code-only. The facade's `supported_verbs()` advertises
// it (only for non-Local sandboxes), and the OutEvent exists — but a portable app
// written against `Harness` alone has no method for it. To use it, drop through
// `raw()` to the concrete provider, then drive the live session and send the
// Claude-specific verb. On Pi, `raw()` yields a PiHarness whose supported_verbs
// EXCLUDES RewindFiles, so the same code is statically fine but the verb would be
// rejected/ignored at the channel — the runtime-promise-vs-parity tension, made
// an explicit door instead of a silent drop.
// ============================================================================

function RewindIfPossible(id: string, to_msg: string, io: Channel) -> HarnessTranscript throws baml.errors.LlmClient {
  let p = CodeAgent();

  // Step down to the concrete runtime via the escape hatch.
  let h: Harness = match (p) {
    let agent: HarnessAgent => agent.raw(),     // <- the documented escape hatch
    _ => throw baml.errors.LlmClient { message: "client is not a harness" },
  };

  // Negotiate: does THIS runtime back rewind at all? (Claude/Container: yes.
  // Pi or Claude/Local: no.) supported_verbs is the honest gate.
  let can_rewind = match (h) {
    let cp: ControlPlane => verb_supported(cp.supported_verbs(), reflect.type_of<RewindFiles>()),
    _ => false,
  };
  if (!can_rewind) {
    throw baml.errors.LlmClient { message: "this runtime cannot rewind files; only Claude Code in a non-Local sandbox can" };
  }

  // It can. Reattach the live session and send the runtime-specific verb over the
  // channel. RewindFiles is NOT part of the portable `Harness` surface — it only
  // exists as an OutEvent the concrete runtime honors.
  io.on(ev => {
    match (ev) {
      let rr: RewindResult => { baml.io.print("[reverted] " + rr.reverted_paths.len() + " files"); },
      _ => {},
    }
  });
  let sh = SessionHandle { id: id, runtime: "claude-code", _ref: rehydrate_ref(id) };
  io.send(RewindFiles { to_user_message_id: to_msg, dry_run: false });
  io.send(EndSession {});
  h.attach(sh, io)
}


// ---- small helpers ----------------------------------------------------------

function drain_to_transcript(evs: baml.llm.Stream<InEvent, HarnessTranscript>)
    -> HarnessTranscript throws baml.errors.LlmClient { $rust_io_function }

function verb_supported(verbs: type[], want: type) -> bool {
  for (let v in verbs) { if (v == want) { return true; } }
  false
}

function rehydrate_ref(id: string) -> $rust_type throws baml.errors.LlmClient { $rust_io_function }
