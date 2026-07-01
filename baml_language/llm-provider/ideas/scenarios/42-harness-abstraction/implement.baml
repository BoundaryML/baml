// ============================================================================
// 42-harness-abstraction / implement.baml   (std-lib / library-author code)
//
// Scenario 37 modeled ONE harness (ClaudeCode) and proved a harness is a
// `Provider` with a `Realtime` control plane. THIS scenario is the layer the
// Vercel AI SDK v7 calls `HarnessAgent`: ONE agent interface over MANY runtimes
// (Claude Code / Codex / Pi), whose output flows into the SAME streaming/UI
// surfaces as a plain model call — because both are `Provider`s.
//
// The whole bet of the proposal is that this layer is *not new machinery*: a
// "HarnessAgent over claudeCode" is just a Provider that wraps another Provider,
// exactly like Fallback/Retry/Cache. So most of this file is combinators + two
// concrete runtimes. What is genuinely net-new and where the model strains:
//
//   (1) Session lifecycle across a SERVER BOUNDARY (createSession / detach /
//       resume / stop / destroy). A session owns a sandbox + child process that
//       must be PARKED or RECLAIMED across stateless request handlers. The
//       proposal's `Realtime.run(prompt, io) -> Transcript` is a single
//       in-process duplex call; it has NO vocabulary for "park this and pick it
//       up in another request." That is server-authoritative state, and it is
//       net-new (`SessionHandle` + `SessionService`).
//
//   (2) Lossy normalization. Claude Code has `rewindFiles`; Pi has `steer` (now)
//       and `followUp` (queued) and NO rewind. There is no common method. The
//       uniform surface (`Harness`) can only carry the INTERSECTION; the rest
//       rides an explicit ESCAPE HATCH (`raw()` down to the concrete runtime).
//
//   (3) Harness output -> the same stream type as a plain call. The adapter's
//       job is to turn a runtime's native event log into `baml.llm.Stream`, so
//       a heavy runtime plugs into the same UI surface as `streamText`.
// ============================================================================


// ----------------------------------------------------------------------------
// (0) Reused verbatim from 37-harness-basics (NOT redefined here):
//   - enum Sandbox { Local | Container | Virtual }
//   - enum PermissionMode { Default | AcceptEdits | Plan | BypassPermissions }
//   - the OutEvent control-plane union (Steer | FollowUp | Interrupt | SetModel
//     | SetPermMode | RewindFiles | StopTask | EndSession)
//   - the InEvent stream union (TextDelta | ToolStarted | ToolFinished |
//     PermissionAsk | TurnDone | RewindResult | SessionError)
//   - class HarnessTranscript { user_message_ids: string[], final_text: string,
//     _raw: $rust_type }
//   - class ClaudeCode / class PiAgent (Provider + Realtime + ControlPlane)
//   - interface ControlPlane requires Realtime { supported_verbs() -> type[] }
// We BUILD ON those. This file adds the cross-runtime abstraction over them.


// ----------------------------------------------------------------------------
// (1) NET-NEW capability: Harness.
//
// `Realtime` (canonical) already gives us a driven duplex session over a
// handed-in Channel. But "a HarnessAgent" is more specific than "a Realtime
// provider": it owns a *session* that survives ACROSS calls and across server
// boundaries. A voice model is Realtime-but-not-Harness (its session dies with
// the socket). A coding runtime is both.
//
// So `Harness requires Realtime` adds exactly ONE thing Realtime lacks: the
// ability to mint a session handle that can be parked and resumed. Everything
// else (driving the live channel, supported_verbs) it inherits.
//
// The session is the INTERSECTION the adapter can promise across runtimes:
// create / attach a live channel / detach / resume / stop / destroy. That set is
// the common denominator the background's `HarnessAgent` actually normalizes.
// ----------------------------------------------------------------------------

// An opaque, runtime-owned reference to a parked session. It is NOT the live
// process — it is the *coordinates* to find it again (e.g. cwd + JSONL path for
// Claude Code, a session-tree node id for Pi, a Durable Object id for Flue).
// Crucially `_ref` is host-opaque: the BAML layer never interprets it, exactly
// like ToolCall.id is opaque to the tool loop.
class SessionHandle {
  id: string                 // stable id the app stores in its DB / cookie
  runtime: string            // "claude-code" | "pi" | "codex" — for routing on resume
  _ref: $rust_type           // host coordinates to re-find the parked session
}

// The lifecycle verbs that MUST live out-of-band of the live channel, because
// they cross a request boundary: a handler in request A creates the session, a
// totally different handler in request B resumes it. You cannot model that with a
// single `Realtime.run(io)` call — `io` only exists within one request.
// NET-NEW capability-error channel: HarnessError.
//
// The Harness lifecycle (create/attach/detach/stop/destroy) is a capability
// beyond call/stream/realtime/tools, so per the error model it gets its OWN
// capability-error interface rather than reusing CallError. Its classifiers
// reflect the failure modes a cross-request session layer actually triages:
// the live runtime behind a stored handle was reclaimed (is_session_expired),
// the sandbox/process leg failed (is_runtime_error). (Declared in baml.errors;
// shown here as the surface this scenario contributes — flagged net-new in
// evaluation.md (b).)
interface baml.errors.HarnessError {
  function is_session_expired(self) -> bool
  function is_runtime_error(self) -> bool
}

interface Harness requires Realtime {
  // Mint a parked session WITHOUT driving it yet. Spawns/leases the runtime,
  // returns coordinates. The live channel comes later, on attach.
  function create_session(self, opts: SessionOpts) -> SessionHandle
      throws baml.ExtendUnknownError<baml.errors.HarnessError>

  // Resume a parked session and DRIVE it over a freshly-handed-in channel. This
  // is `Realtime.run` but keyed by a handle instead of starting fresh: the prior
  // history/sandbox/workdir are reattached. `io` is a NEW channel for THIS request.
  function attach(self, h: SessionHandle, io: Channel) -> HarnessTranscript
      throws baml.ExtendUnknownError<baml.errors.HarnessError>

  // Park without destroying: persist history, relinquish the live channel, leave
  // the sandbox claimable. Returns the (possibly-updated) handle to store.
  function detach(self, h: SessionHandle) -> SessionHandle
      throws baml.ExtendUnknownError<baml.errors.HarnessError>

  // Halt an in-flight turn but KEEP the session (so it can be attached again).
  function stop(self, h: SessionHandle) -> null
      throws baml.ExtendUnknownError<baml.errors.HarnessError>

  // Tear down: kill the child process, release the sandbox, drop the transcript.
  function destroy(self, h: SessionHandle) -> null
      throws baml.ExtendUnknownError<baml.errors.HarnessError>
}

// Config handed to create_session. The bound provider fields (model/sandbox/...)
// already live on the concrete provider; SessionOpts is the PER-SESSION overrides
// the background calls out (workdir, resume-from, initial perm mode).
class SessionOpts {
  cwd: string?
  resume_from: string?           // a prior message id / fork point, runtime-specific
  permission_mode: PermissionMode?
}


// ----------------------------------------------------------------------------
// (2) NET-NEW capability: AgentStreaming.
//
// "Harness output flows into the SAME streaming/UI surfaces as a plain call."
// Canonical `Streaming.stream<TStream, TFinal>` yields `baml.llm.Stream` — the
// exact type `useChat`/`toUIMessageStream` consume. A harness produces a rich
// InEvent log, NOT a token stream. The adapter's job is to PROJECT the InEvent
// log onto `baml.llm.Stream` so the UI can't tell a Claude Code turn from a
// `streamText` turn.
//
// We make a harness implement the canonical `Streaming` directly (so it drops
// into existing UI code) AND expose the richer event log for clients that want
// tool-call cards. `AgentStreaming` is the refinement that surfaces both.
// ----------------------------------------------------------------------------

interface AgentStreaming requires Streaming {
  // The lossless event log (ToolStarted/PermissionAsk/TurnDone/...). A plain UI
  // ignores this and uses `.stream`; a rich agent UI subscribes to it.
  function events(self, prompt: baml.llm.PromptAst, h: SessionHandle?) -> baml.llm.Stream<InEvent, HarnessTranscript>
      throws baml.ExtendUnknownError<baml.errors.StreamError>
}


// ----------------------------------------------------------------------------
// (3) The unifying facade: HarnessAgent.
//
// THIS is the AI-SDK-v7 `new HarnessAgent({ harness: claudeCode(...) })`. It is
// a Provider that WRAPS a Harness provider and re-exposes a STABLE, normalized
// surface. It forwards by runtime delegation — the same pattern as Fallback —
// so an app can hold `HarnessAgent` and never name the concrete runtime.
//
// The payoff the proposal predicts: because HarnessAgent IS a Provider (it
// `implements` capabilities that `requires Provider` — the marker — so no explicit
// `implements Provider` is needed), the `client → function ... -> Provider` sugar
// covers it. Driving a HarnessAgent over `.live`/`.stream`/`.run_tools` reads the
// same as driving an OpenAI client over its own capabilities.
// ----------------------------------------------------------------------------

class HarnessAgent {
  inner: Harness            // the concrete runtime (ClaudeCode, PiAgent, ...)

  // NO `implements Provider { call }`. `Provider` is now a bare MARKER
  // (../provider-as-marker.md): a harness has no honest request/response `call` to
  // forward. Its inner runtime is a Harness/Realtime, not an HttpProvider — the
  // old `inner.call<T>()` was the degenerate "run the whole subprocess and hope"
  // that the marker revision deletes. HarnessAgent still IS a Provider (it
  // `implements` capabilities that `requires Provider`), so no explicit
  // `implements Provider` is needed; it exposes only its REAL interactions below
  // (Streaming/AgentStreaming/Realtime/Harness/ControlPlane). An app that wants a
  // single value drains the stream/event log (see usage.baml `Summarize`).

  // -- Streaming: project the runtime's event log onto baml.llm.Stream. --------
  // This is the "indistinguishable from streamText at the UI" move. We pull the
  // InEvent stream and map TextDelta -> stream items, dropping the control events
  // a plain UI doesn't render. The result is the SAME type a chat model emits.
  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      // inner is Realtime/Harness, not necessarily classic Streaming, so we lift
      // its event log into a token-shaped stream. project_text_stream keeps only
      // assistant text deltas and the final transcript -> TFinal.
      project_text_stream<TStream, TFinal>(self.inner.events(prompt, null))
    }
  }

  // -- AgentStreaming: hand back the FULL event log for rich agent UIs. --------
  implements AgentStreaming {
    function events(self, prompt: baml.llm.PromptAst, h: SessionHandle?)
        -> baml.llm.Stream<InEvent, HarnessTranscript>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      self.inner.events(prompt, h)
    }
  }

  // -- Realtime: forward the driven duplex straight through. -------------------
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      self.inner.run(prompt, io)
    }
  }

  // -- Harness: forward session lifecycle, stamping the runtime tag so a stored
  //    handle can be routed back to the right runtime on resume (the background's
  //    "cwd-keyed storage orphans transcripts when a project moves" problem made
  //    explicit: the handle carries its own runtime identity). -----------------
  implements Harness {
    function create_session(self, opts: SessionOpts) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      self.inner.create_session(opts)
    }
    function attach(self, h: SessionHandle, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      self.inner.attach(h, io)
    }
    function detach(self, h: SessionHandle) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      self.inner.detach(h)
    }
    function stop(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      self.inner.stop(h)
    }
    function destroy(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      self.inner.destroy(h)
    }
  }

  // -- ControlPlane: forward verb negotiation so a UI greys out what THIS
  //    runtime can't back (Claude Code shows rewind, Pi doesn't). --------------
  implements ControlPlane {
    function supported_verbs(self) -> type[] {
      // inner is Realtime; if it also advertises a verb set, surface it; else
      // assume the conservative intersection (steer/followup/end only).
      match (self.inner) {
        let cp: ControlPlane => cp.supported_verbs(),
        _ => [
          reflect.type_of<Steer>(),
          reflect.type_of<FollowUp>(),
          reflect.type_of<EndSession>(),
        ],
      }
    }
  }

  // -- THE ESCAPE HATCH for lossy normalization. -------------------------------
  // rewindFiles (Claude) and steer-now (Pi) have NO common method. The uniform
  // surface (events/run/supported_verbs) carries the intersection. Anything
  // runtime-specific rides `raw()`: hand back the concrete provider so the app
  // can `match` it down and use a feature the facade can't name. This is the
  // runtime-promise-vs-feature-parity tension, made an explicit door rather than
  // a silent drop. (Mirrors the background: "route it through an escape hatch.")
  function raw(self) -> Harness { self.inner }
}


// ----------------------------------------------------------------------------
// (4) Concrete runtime adapters: the Harness/AgentStreaming impls.
//
// ClaudeCode and PiAgent already implement Provider + Realtime + ControlPlane
// (37). Here we add the cross-boundary session lifecycle (Harness) and the event
// projection (AgentStreaming). The DIVERGENCE the abstraction must absorb is
// inside these bodies: Claude Code parks to on-disk JSONL keyed by cwd; Pi parks
// to a node in an in-memory/persistent session TREE. Same `SessionHandle`
// surface, totally different `_ref`.
// ----------------------------------------------------------------------------

// We extend ClaudeCode (from 37) with the new capabilities. (In real BAML the
// `implements` blocks live in the original class; shown here as the additive
// surface this scenario contributes.)
class ClaudeCodeHarness {
  model: string?
  sandbox: Sandbox
  permission_mode: PermissionMode
  allowed_tools: string[]?
  cwd: string?

  // Realtime + ControlPlane bodies are identical to 37's ClaudeCode; elided here
  // to keep the focus on the cross-runtime additions. We re-declare the signatures
  // so this file type-checks standalone. NO `implements Provider { call }`: a
  // harness has no honest request/response `call` (the old body was the degenerate
  // "run the whole subprocess and hope"). Under the marker revision the class still
  // IS a Provider via `Realtime`/`Harness` (both `requires Provider`); its real
  // interaction is the driven session + the lifecycle below.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> { $rust_io_function }
  }
  implements ControlPlane {
    function supported_verbs(self) -> type[] {
      let base = [
        reflect.type_of<Steer>(), reflect.type_of<FollowUp>(),
        reflect.type_of<Interrupt>(), reflect.type_of<SetModel>(),
        reflect.type_of<SetPermMode>(), reflect.type_of<StopTask>(),
        reflect.type_of<EndSession>(),
      ];
      match (self.sandbox) {
        Sandbox.Local => base,                                       // Local can't reliably revert fs
        _ => base.concat([reflect.type_of<RewindFiles>()]),         // ONLY Claude has rewind
      }
    }
  }

  implements Harness {
    // Claude Code's session is JSONL on disk keyed by cwd. create_session leases
    // a sandbox + workdir and writes the transcript file; the handle's _ref is
    // (cwd, jsonl_path). The background's "cwd-keyed storage orphans transcripts
    // when a project moves" risk lives entirely in this host body.
    function create_session(self, opts: SessionOpts) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    // attach = `claude --resume <session_id> --cwd <cwd>`, then drive `io` exactly
    // like run(). The prior JSONL is replayed so history/workdir come back.
    function attach(self, h: SessionHandle, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    // detach = flush JSONL, kill the child but KEEP the file + sandbox lease so a
    // later request can --resume. Returns the handle (possibly with an updated
    // last-message id baked into _ref).
    function detach(self, h: SessionHandle) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    function stop(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function                                              // SIGINT the current turn, keep session
    }
    function destroy(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function                                              // delete JSONL, release sandbox
    }
  }

  implements AgentStreaming {
    function events(self, prompt: baml.llm.PromptAst, h: SessionHandle?)
        -> baml.llm.Stream<InEvent, HarnessTranscript>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      // Spawn (or --resume h), then pump the subprocess's JSONL event log up as
      // InEvents. The host body translates Claude's native stream-json lines into
      // TextDelta/ToolStarted/ToolFinished/PermissionAsk/TurnDone.
      $rust_io_function
    }
  }
}

// Pi: divergent on EVERYTHING the abstraction tries to hide.
//   - NO rewind, NO interrupt verb. supported_verbs is a strict SUBSET.
//   - Session is a TREE NODE (parent-child fork), not a flat JSONL file. So its
//     `_ref` is a node id, and `resume_from` means "fork at this node," which has
//     no Claude analog.
//   - `steer` is immediate-mid-turn; `followUp` is queued. Both ride the SAME
//     Steer/FollowUp OutEvents (the 37 vocabulary already split them), so the
//     live-driving surface is portable even though the lifecycle isn't.
class PiHarness {
  model: string
  cwd: string?

  // NO `implements Provider { call }` — same as ClaudeCodeHarness: a harness has no
  // honest one-shot request/response. PiHarness IS a Provider via Realtime/Harness
  // (marker), and exposes only its real interactions below.
  implements Realtime {
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> { $rust_io_function }
  }
  implements ControlPlane {
    function supported_verbs(self) -> type[] {
      // Pi: steer (now) + followUp (queued) + end. No interrupt, no rewind,
      // no setModel mid-run, no stopTask. The facade's UI greys out the rest.
      [reflect.type_of<Steer>(), reflect.type_of<FollowUp>(), reflect.type_of<EndSession>()]
    }
  }

  implements Harness {
    // Pi parks to a node in a session tree. create_session mints a node; if
    // opts.resume_from is set it FORKS at that node (cheap pointer-to-parent,
    // unlike Claude's copy). The _ref is the node id.
    function create_session(self, opts: SessionOpts) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    function attach(self, h: SessionHandle, io: Channel) -> HarnessTranscript
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    function detach(self, h: SessionHandle) -> SessionHandle
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function
    }
    function stop(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      // Pi has NO interrupt verb. The best stop() can do is stop subscribing and
      // let the in-flight turn settle; it CANNOT halt mid-turn. The uniform
      // `stop` therefore means something WEAKER on Pi — a lossy normalization the
      // facade can't fix. We surface it honestly: stop returns, but the turn may
      // still complete server-side. (Documented divergence, not a lie.)
      $rust_io_function
    }
    function destroy(self, h: SessionHandle) -> null
        throws baml.ExtendUnknownError<baml.errors.HarnessError> {
      $rust_io_function                                              // drop the node + children
    }
  }

  implements AgentStreaming {
    function events(self, prompt: baml.llm.PromptAst, h: SessionHandle?)
        -> baml.llm.Stream<InEvent, HarnessTranscript>
        throws baml.ExtendUnknownError<baml.errors.StreamError> {
      // Pi's `subscribe` event stream, mapped onto the same InEvent union. Pi
      // emits no PermissionAsk (no built-in permission gate) — that variant just
      // never appears. Same union, narrower inhabitation.
      $rust_io_function
    }
  }
}


// ----------------------------------------------------------------------------
// (5) NET-NEW: SessionService — server-authoritative session state.
//
// The lifecycle methods above PARK a session, but SOMETHING has to remember the
// handle across requests, and the sandbox-reclaim policy (TTL eviction, orphan
// GC) is server state the BAML layer cannot own. This is the durable-instance
// problem the background flags. We model the storage of handles (not the live
// process) as a pluggable store, exactly like 17's SessionStore — but we are
// HONEST that the live runtime behind the handle is server-authoritative and may
// be GC'd out from under us (attach can throw SessionExpired).
// ----------------------------------------------------------------------------

// net-new CONCRETE error: the stored handle outlived its runtime. It is a domain
// failure the harness layer raises on purpose (the runtime was reclaimed), so it
// is a typed error the app branches on — NOT a foreign error routed to
// UnknownError. It surfaces both from the store's `get` AND mid-attach, so it
// implements baml.errors.HarnessError; is_session_expired -> true is the bit
// callers triage on.
class SessionExpired {
  id: string
  implements baml.errors.HarnessError {
    function is_session_expired(self) -> bool { return true; }
    function is_runtime_error(self)   -> bool { return false; }
  }
}

interface SessionService {
  function put(self, h: SessionHandle) -> null throws baml.errors.Io
  function get(self, id: string) -> SessionHandle throws baml.errors.Io | SessionExpired
  function drop(self, id: string) -> null throws baml.errors.Io
}

// The default: handles in a host-held map with a TTL. The map holds COORDINATES;
// the live sandboxes live in the runtime, reclaimed on their own clock. So a
// handle can be present in the store yet its runtime already evicted — get()
// returns it, but the subsequent attach() is what discovers SessionExpired. The
// store cannot know; only the runtime is authoritative. (This split is the whole
// point — see evaluation (c).)
class HandleStore {
  _cells: $rust_type                                 // host map<string, SessionHandle> + ttl
  function new() -> HandleStore { HandleStore { _cells: $rust_io_function } }

  implements SessionService {
    function put(self, h: SessionHandle) -> null throws baml.errors.Io { $rust_io_function }
    function get(self, id: string) -> SessionHandle throws baml.errors.Io | SessionExpired { $rust_io_function }
    function drop(self, id: string) -> null throws baml.errors.Io { $rust_io_function }
  }
}


// ----------------------------------------------------------------------------
// (6) The cross-request driver combinator: ServerSession.
//
// Two stateless handlers, one logical session. Handler A: create + drive + detach
// + persist handle. Handler B (later request): load handle + attach + drive +
// detach. ServerSession bundles the handle store + the harness so an app author
// writes the two halves without re-deriving the dance each time. Pure library
// glue over the Harness + SessionService primitives — no host body.
// ----------------------------------------------------------------------------

// A concrete, classified HarnessError the cross-request driver raises on purpose
// when a lifecycle leg fails (attach/persist/reattach). It is a runtime-leg
// failure, distinct from SessionExpired (which means the runtime is gone). The
// driver throws it directly, so it stays a known HarnessError on the channel and
// the trailing UnknownError normalization never has to fire for these.
class HarnessRuntimeError {
  message: string
  implements baml.errors.HarnessError {
    function is_session_expired(self) -> bool { return false; }
    function is_runtime_error(self)   -> bool { return true; }
  }
}

class ServerSession {
  agent: HarnessAgent
  service: SessionService

  // Handler A: start a fresh logical session, drive it over THIS request's io,
  // then PARK it and persist the handle for the next request to find.
  function open(self, prompt: baml.llm.PromptAst, opts: SessionOpts, io: Channel)
      -> SessionHandle throws baml.ExtendUnknownError<baml.errors.HarnessError> {
    let h = self.agent.create_session(opts);
    // drive this request's turn(s)
    self.agent.attach(h, io) catch (e) {
      _ => { self.agent.destroy(h) catch (e2) { _ => {} }; throw HarnessRuntimeError { message: "open turn failed" }; }
    };
    // park: relinquish the live channel, keep the sandbox claimable
    let parked = self.agent.detach(h);
    self.service.put(parked) catch (e) {
      _ => { throw HarnessRuntimeError { message: "could not persist session handle" }; }
    };
    return parked;
  }

  // Handler B: a DIFFERENT request resumes. Load the handle, attach a NEW channel,
  // drive, re-park. The SessionExpired arm is the server-authoritative truth: the
  // runtime may have been reclaimed while the user was away, and there is nothing
  // the BAML layer could have done to prevent it — only to handle it.
  function resume(self, id: string, io: Channel) -> SessionHandle
      throws baml.ExtendUnknownError<baml.errors.HarnessError> {
    let h = self.service.get(id) catch (e) {
      // SessionExpired is already a HarnessError; rethrow it so the app can branch
      // on is_session_expired() rather than parsing a string.
      let exp: SessionExpired => { throw exp; },
      _ => { throw HarnessRuntimeError { message: "session store unavailable" }; },
    };
    self.agent.attach(h, io) catch (e) {
      // attach is the SECOND place expiry surfaces: the handle was stored but the
      // sandbox is gone. The store said yes; the runtime says no. Authoritative.
      // SessionExpired is a concrete HarnessError thrown on purpose here — preserve
      // it (drop the dead handle, then rethrow) so the consumer keeps the
      // is_session_expired() bit instead of having it flattened into a runtime leg.
      let exp: SessionExpired => { self.service.drop(id) catch (e2) { _ => {} }; throw exp; },
      _ => { self.service.drop(id) catch (e2) { _ => {} }; throw HarnessRuntimeError { message: "session " + id + " could not be reattached" }; }
    };
    let parked = self.agent.detach(h);
    self.service.put(parked) catch (e) { _ => {} };
    return parked;
  }

  // Explicit teardown from a /delete handler.
  function close(self, id: string) -> null
      throws baml.ExtendUnknownError<baml.errors.HarnessError> {
    let h = self.service.get(id) catch (e) { _ => { return null; } };  // already gone: nothing to do
    self.agent.destroy(h) catch (e) { _ => {} };
    self.service.drop(id) catch (e) { _ => {} };
    return null;
  }
}


// ----------------------------------------------------------------------------
// (7) Host-boundary helpers
// ----------------------------------------------------------------------------

// Project a harness InEvent log onto a token-shaped baml.llm.Stream, keeping only
// assistant TextDeltas as stream items and the settled HarnessTranscript as
// TFinal. THIS is the "indistinguishable from streamText at the UI" translation;
// it is a genuine host boundary (it bridges two opaque stream representations).
function project_text_stream<TStream, TFinal>(events: baml.llm.Stream<InEvent, HarnessTranscript>)
    -> baml.llm.Stream<TStream, TFinal>
    throws baml.ExtendUnknownError<baml.errors.StreamError> { $rust_io_function }
