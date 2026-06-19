// ============================================================================
// Suspend / resume & human-in-the-loop — LIBRARY / STD-LIB AUTHOR CODE
// ============================================================================
//
// Builds ON the canonical spine (Provider, HttpProvider, Tools, ...). We do NOT
// redefine those. We introduce:
//
//   * `Suspend<R>`            — a host-thrown signal carrying a resume schema + an
//                               approver-facing payload (the suspendSchema seam).
//   * `Suspendable`           — a capability interface `requires Provider`: a provider
//                               whose `call` may legally suspend, and that can be
//                               re-entered with resume data.
//   * `Snapshot`              — opaque, host-owned, serializable run state. The
//                               teaching twin of the opaque `Transcript`/`PromptAst`:
//                               the *only* thing allowed to cross the wait.
//   * `Durable`               — a combinator that turns a Suspendable provider into a
//                               persisted run: start() -> RunHandle, resume() exactly-once.
//   * `ApprovalGate`          — a Tools-layer combinator: gate named tools on human
//                               sign-off, suspending instead of auto-firing.
//
// The hard parts the background names live in the provider/host bodies below:
//   - serializing live state (no closures/sockets/streams) -> $rust_type Snapshot
//   - schema drift across the wait                          -> version tag on Snapshot
//   - exactly-once resume                                   -> claim() at the store
// ============================================================================

// ----------------------------------------------------------------------------
// The suspend signal. This is NOT an error in the "call failed" sense — it is a
// control-flow escape that the Durable combinator catches. It rides the SAME
// `throws` channel the spine already uses, so a Suspendable provider's `call`
// signature widens its error type to include it. `R` is the resume schema as a
// first-class `type` (parameters: type, exactly like `Tool`).
// ----------------------------------------------------------------------------
class Suspend {
  // what the approver/human is shown — the Mastra `suspendSchema` payload, as data
  payload: unknown
  // the schema the resume value MUST validate against — Mastra `resumeSchema`
  resume_schema: type
  // a stable, provider-chosen label for WHICH suspension point this is, so a
  // workflow with several suspend() points resumes at the right one.
  point: string
}

// ----------------------------------------------------------------------------
// Snapshot: opaque, host-owned, SERIALIZABLE run state. Mirrors PromptAst's
// `_data: $rust_type` discipline. The host guarantees it contains no closures,
// no sockets, no live streams — only plain data + a schema-version tag. The
// BAML side can never reach inside it; it can only hand it back to `reenter`.
// ----------------------------------------------------------------------------
class Snapshot {
  // schema/code version captured AT SUSPEND TIME. The host stamps this from the
  // build hash so a Friday deploy can detect a Monday snapshot (drift, below).
  version: string
  point: string                 // which Suspend.point produced it
  _state: $rust_type            // opaque frozen continuation state, plain-data only
}

// ----------------------------------------------------------------------------
// The capability. A Suspendable provider is one whose `call` may suspend, and
// which can be RE-ENTERED with a snapshot + resume value to drive to completion
// (or suspend again). It is NOT a host boundary on its own — it is a capability
// any provider (HTTP chat model, harness, tool loop) can layer on.
//
// `reenter` is the resume half of `call`: same generic output `T`, but it takes
// the snapshot the host persisted plus the human's resume value.
// ----------------------------------------------------------------------------
interface Suspendable requires Provider {
  // produce a fresh snapshot if `call` (or reenter) suspends; otherwise return T.
  // We model "may suspend OR finish" as a union, the Iterator.next shape again:
  //   T          -> done
  //   Suspend    -> paused, here is what to persist + show the human
  function start<T>(self, prompt: baml.llm.PromptAst) -> T | Suspend throws Error

  // re-enter a persisted run. `resume` is the human's answer (validated by the
  // caller against snap-recovered resume_schema BEFORE this is called).
  function reenter<T>(self, snap: Snapshot, resume: unknown) -> T | Suspend throws Error

  // the host freezes the just-suspended continuation into a Snapshot. This is the
  // boundary that "serializes live state": it is the host's job to refuse if the
  // live state holds a non-serializable thing (open stream/socket/closure).
  function freeze(self, s: Suspend) -> Snapshot throws Error
}

// ============================================================================
// A concrete Suspendable provider: an HTTP chat model whose tool loop can pause
// for human approval. We layer Suspendable ON an HttpProvider + Tools provider.
// ============================================================================
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // Anthropic puts the output schema in a FORCED tool (one of the four encodings).
      // This is the real divergence the spine keeps inside build_request.
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())
    }
  }

  implements Tools {
    // Anthropic correlates tool results POSITIONALLY by tool_use id; that id-vs-name
    // bookkeeping stays inside begin/step/submit, below the ToolCall seam.
    type Transcript = AnthropicTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      $rust_io_function
    }
    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      $rust_io_function
    }
    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // matches each ToolResult.id back to the open tool_use block, in Anthropic's
      // required positional order. Opaque to the loop above.
      $rust_io_function
    }
  }

  // The suspend capability. Crucially this is implemented in terms of the SAME
  // tool loop: a run "suspends" exactly when `step` yields a tool call whose tool
  // is gated on approval. The host owns turning the live transcript into a
  // serializable Snapshot (freeze) and back (reenter).
  implements Suspendable {
    function start<T>(self, prompt: baml.llm.PromptAst) -> T | Suspend throws baml.errors.LlmClient {
      // host drives begin/step until a gated tool call appears; if it does, it
      // returns Suspend{ payload: the pending call, resume_schema, point }.
      $rust_io_function
    }
    function reenter<T>(self, snap: Snapshot, resume: unknown) -> T | Suspend throws baml.errors.LlmClient {
      // host re-hydrates the AnthropicTranscript FROM snap._state, injects `resume`
      // as the approved/denied tool_result (or as resumeData), and continues the
      // loop. May suspend again at the next gate.
      $rust_io_function
    }
    function freeze(self, s: Suspend) -> Snapshot throws baml.errors.LlmClient {
      // serialize the live transcript + pending point into Snapshot._state. REFUSES
      // (throws) if the continuation captured a non-serializable resource. Stamps
      // the current build version onto Snapshot.version.
      $rust_io_function
    }
  }
}

// Anthropic's opaque, provider-owned transcript (associated type for Tools).
class AnthropicTranscript {
  _data: $rust_type
}

// ============================================================================
// A second Suspendable provider where the background shows divergence: a harness
// (Claude Code) is a Provider with NO HttpProvider, whose suspension is a
// long-lived subprocess parking itself rather than an HTTP tool gate. Same
// Suspendable interface; completely different body. Proves portability of the
// capability across transport.
// ============================================================================
class ClaudeCode {
  model: string?
  sandbox: Sandbox
  allowed_tools: string[]?
  permission_mode: string?

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function                          // drives the spawned `claude` subprocess
    }
  }

  implements Suspendable {
    function start<T>(self, prompt: baml.llm.PromptAst) -> T | Suspend throws baml.errors.LlmClient {
      // the subprocess runs until it hits a permission prompt (e.g. a Bash command
      // the sandbox won't auto-approve). The host SUSPENDS the subprocess (it does
      // not keep it alive — it serializes the session id + cwd diff), and returns
      // Suspend{ payload: the proposed action, resume_schema: ApprovalDecision, ... }.
      $rust_io_function
    }
    function reenter<T>(self, snap: Snapshot, resume: unknown) -> T | Suspend throws baml.errors.LlmClient {
      // re-attach to / re-spawn the session from snap._state, feed the decision,
      // continue. A fresh process — the original may be long gone.
      $rust_io_function
    }
    function freeze(self, s: Suspend) -> Snapshot throws baml.errors.LlmClient {
      // the hard one for a harness: the live socket/subprocess CANNOT be serialized.
      // freeze persists only the session id + replayable cwd state and throws if the
      // session is unrecoverable. This is exactly the "no sockets/streams" rule.
      $rust_io_function
    }
  }
}

// ============================================================================
// Durable: the combinator that turns a Suspendable provider into a PERSISTED run.
// This is where exactly-once resume and schema-drift detection live. It wraps a
// Suspendable and a store; it is itself a plain Provider so it still composes.
// ============================================================================

// A persistent handle to a (possibly dormant) run. Plain data — it is just an id.
class RunHandle {
  run_id: string
  status: string                 // "running" | "suspended" | "done" | "failed"
  // present only when status == "suspended": what to show the human + how to validate.
  pending: Suspend?
}

interface RunStore {
  // exactly-once primitive: atomically claim the run for resume. Returns the snapshot
  // iff THIS caller won the claim; null if already claimed/resumed (double-click,
  // two replicas). Backed by a conditional write (compare-and-set on a claim token).
  function claim(self, run_id: string, token: string) -> Snapshot? throws baml.errors.Io
  function put(self, run_id: string, snap: Snapshot, pending: Suspend) -> null throws baml.errors.Io
  function release(self, run_id: string, token: string, status: string) -> null throws baml.errors.Io
  function load_handle(self, run_id: string) -> RunHandle throws baml.errors.Io
}

class Durable {
  inner: Suspendable
  store: RunStore
  // the build version this process is running; compared to Snapshot.version on resume.
  build_version: string

  // NOTE: Durable is a Provider (it forwards plain .call), but suspend/resume need a
  // run id, so the real surface is start()/resume() statics below, not bare .call.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // a non-suspending convenience: run to completion or fail loudly if it parks.
      let run_id = baml.sys.uuid();
      match (self.start_run<T>(run_id, prompt)) {
        let v: T => { return v; },
        let h: RunHandle => {
          throw baml.errors.LlmClient { message: "run suspended; use Durable.resume (run_id=" + run_id + ")" };
        },
      }
    }
  }

  // START a run. Persists a snapshot if it suspends; returns either the value or a
  // suspended RunHandle the caller can hand to a UI.
  function start_run<T>(self, run_id: string, prompt: baml.llm.PromptAst) -> T | RunHandle throws baml.errors.LlmClient {
    match (self.inner.start<T>(prompt)) {
      let v: T => {
        return v;
      },
      let s: Suspend => {
        let snap: Snapshot = self.inner.freeze(s);
        self.store.put(run_id, snap, s) catch (e) { _ => { throw baml.errors.LlmClient { message: "persist failed" }; } };
        return RunHandle { run_id: run_id, status: "suspended", pending: s };
      },
    }
  }

  // RESUME a run. `resume` is the human's answer. EXACTLY-ONCE is enforced by claim():
  // if two replicas / a double-click race here, only the claim winner proceeds.
  function resume_run<T>(self, run_id: string, resume: unknown) -> T | RunHandle throws baml.errors.LlmClient {
    let token = baml.sys.uuid();
    let claimed: Snapshot? = self.store.claim(run_id, token)
      catch (e) { _ => { throw baml.errors.LlmClient { message: "claim failed" }; } };

    match (claimed) {
      null => {
        // lost the race (already resumed) — return the current handle, do NOT re-run.
        return self.store.load_handle(run_id)
          catch (e) { _ => { throw baml.errors.LlmClient { message: "load failed" }; } };
      },
      let snap: Snapshot => {
        // SCHEMA DRIFT GATE: a Monday snapshot resuming on a Friday build. We refuse
        // to silently re-enter a continuation produced by different code/schema.
        if (snap.version != self.build_version) {
          self.store.release(run_id, token, "failed")
            catch (e) { _ => {} };
          throw baml.errors.LlmClient {
            message: "snapshot version " + snap.version + " != build " + self.build_version + " (schema drift)"
          };
        }
        match (self.inner.reenter<T>(snap, resume)) {
          let v: T => {
            self.store.release(run_id, token, "done") catch (e) { _ => {} };
            return v;
          },
          let s: Suspend => {
            // suspended AGAIN at the next gate. Persist the new snapshot, release the
            // claim back to suspended so the next human can resume.
            let snap2: Snapshot = self.inner.freeze(s);
            self.store.put(run_id, snap2, s) catch (e) { _ => { throw baml.errors.LlmClient { message: "persist failed" }; } };
            self.store.release(run_id, token, "suspended") catch (e) { _ => {} };
            return RunHandle { run_id: run_id, status: "suspended", pending: s };
          },
        }
      },
    }
  }
}

// ============================================================================
// ApprovalGate: a Tools-layer combinator that turns "run this tool" into "suspend
// for human approval, then run it." It wraps a Tools provider and the set of gated
// tool names. It is the BAML-side translation of Pydantic AI's
// `@agent.tool(requires_approval=True)`: the model never auto-fires the tool.
//
// It is itself Suspendable: begin/step/submit drive normally until `step` yields a
// gated call, at which point `start`/`reenter` surface a Suspend instead.
// ============================================================================
class ApprovalGate {
  inner: Tools
  gated: string[]                 // tool names that require human sign-off
  // the schema a human's decision must validate against (resumeSchema).
  decision_schema: type

  function is_gated(self, name: string) -> bool {
    for (let g in self.gated) {
      if (g == name) { return true; }
    }
    false
  }

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
    }
  }

  implements Suspendable {
    function start<T>(self, prompt: baml.llm.PromptAst) -> T | Suspend throws baml.errors.LlmClient {
      let t = self.inner.begin<T>(prompt, gated_tools_of(self));
      self.drive<T>(t)
    }

    function reenter<T>(self, snap: Snapshot, resume: unknown) -> T | Suspend throws baml.errors.LlmClient {
      // rebuild the inner transcript from the snapshot, then translate the human's
      // decision into a ToolResult for the gated call and submit it.
      let t: Transcript = rehydrate_transcript(self.inner, snap);
      let approved = decision_is_approved(resume);
      if (approved) {
        // run the gated call for real now, feed its output back as the tool result.
        let pending: ToolCall = pending_call_of(snap);
        let result: ToolResult = run_for_real(pending);
        let t2 = self.inner.submit(t, [result]);
        return self.drive<T>(t2);
      } else {
        // denial is also a tool result: tell the model the human refused.
        let pending: ToolCall = pending_call_of(snap);
        let denied = ToolResult { id: pending.id, output: "denied by human reviewer" };
        let t2 = self.inner.submit(t, [denied]);
        return self.drive<T>(t2);
      }
    }

    function freeze(self, s: Suspend) -> Snapshot throws baml.errors.LlmClient {
      // delegate to the host's transcript-freeze; ApprovalGate adds no live state.
      freeze_transcript(self.inner, s)
    }
  }

  // shared loop: step until done OR until a gated call shows up; on a gated call,
  // emit a Suspend carrying that call as the approver payload.
  function drive<T>(self, t0: Transcript) -> T | Suspend throws baml.errors.LlmClient {
    let t = t0;
    while (true) {
      match (self.inner.step<T>(t)) {
        let value: T => { return value; },
        let calls: ToolCalls => {
          // partition: gated calls suspend; ungated calls run inline.
          let pending: ToolCall? = first_gated(self, calls.calls);
          match (pending) {
            let p: ToolCall => {
              return Suspend {
                payload: p,                          // show the human WHICH action
                resume_schema: self.decision_schema, // how their answer is validated
                point: "approval:" + p.name,
              };
            },
            null => {
              // none gated — dispatch them all inline and continue the loop.
              t = self.inner.submit(t, dispatch_all(calls.calls));
            },
          }
        },
      }
    }
    baml.sys.panic("unreachable")
  }
}

// ----------------------------------------------------------------------------
// Host helpers (genuine boundaries — they touch the opaque transcript/snapshot,
// the real tool dispatcher, or the build hash). Kept as free functions to keep
// the combinator bodies readable.
// ----------------------------------------------------------------------------
function gated_tools_of(g: ApprovalGate) -> Tool[] { $rust_io_function }
function rehydrate_transcript(inner: Tools, snap: Snapshot) -> Transcript throws baml.errors.LlmClient { $rust_io_function }
function freeze_transcript(inner: Tools, s: Suspend) -> Snapshot throws baml.errors.LlmClient { $rust_io_function }
function pending_call_of(snap: Snapshot) -> ToolCall { $rust_io_function }
function run_for_real(call: ToolCall) -> ToolResult throws baml.errors.LlmClient { $rust_io_function }
function dispatch_all(calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient { $rust_io_function }
function first_gated(g: ApprovalGate, calls: ToolCall[]) -> ToolCall? {
  for (let c in calls) {
    if (g.is_gated(c.name)) { return c; }
  }
  null
}
function decision_is_approved(resume: unknown) -> bool { $rust_io_function }

// `Transcript` here names the inner provider's associated Tools.Transcript type;
// the host resolves it through `inner`. (Same opacity discipline as the spine.)
