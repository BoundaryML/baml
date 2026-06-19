// ─────────────────────────────────────────────────────────────────────────────
// Durable execution — library / std-lib-author code.
//
// The central correctness issue (background §4 "What's hard"): a replay-based
// engine assumes a step, re-run with the same inputs, yields the same output.
// An LLM call does NOT. So the model call inside a durable step must be
// RECORDED-and-replayed, never re-sampled. We express that as a combinator that
// wraps a Provider: `Durable` turns `.call` into a checkpointed activity keyed
// by (run, step). On first execution it samples once and records the response;
// on replay it returns the recorded response and never touches the network.
//
// This is structurally a Cache (the canonical combinator) whose key is the
// workflow coordinate rather than the prompt hash, and whose store is the
// engine's own step log. The whole point of durability is that the key is the
// STEP COORDINATE — two different prompts at the same coordinate is a
// determinism violation, and we surface it rather than silently re-sample.
// ─────────────────────────────────────────────────────────────────────────────

// ── Host boundary: a durable engine's step log. ──────────────────────────────
// Temporal/DBOS/Inngest/Restate all expose the same two primitives: ask the
// engine for the recorded result of a step coordinate (replay), or record one
// (first execution). The engine owns persistence, event-sourcing, and the
// at-least-once retry envelope — none of which BAML reimplements.
//
// `StepLog` is the existential the adapter hands us; concrete adapters below.
interface StepLog {
  // Look up a previously-recorded step result by coordinate. Returns null on a
  // genuine first execution (the engine has no checkpoint for this coordinate).
  function recorded(self, coord: StepCoord) -> string? throws baml.errors.Io
  // Record a step result so replay returns it instead of re-executing. The
  // engine persists this in its event log / step table atomically with the
  // step's completion; on the next replay `recorded` returns it.
  function record(self, coord: StepCoord, value: string) -> null throws baml.errors.Io
  // The engine's deterministic clock for this run position (replay-safe). Used
  // so a wrapped provider that wants "now" doesn't read the wall clock in the
  // body (a determinism violation — see §4 "Determinism rules").
  function deterministic_now(self) -> string throws baml.errors.Io
}

// A step coordinate: stable per (run, step) so replay hits the same key. The
// engine assigns the run id; `step` is the position in the workflow body (the
// engine increments it as deterministic step calls execute, exactly like
// Temporal's command sequence id / Inngest's step id).
class StepCoord {
  run: string
  step: string                       // logical name, e.g. "extract" or "charge"
}

// ── The combinator: record-once, replay-forever around any Provider.call. ────
//
// `Durable` is a plain (non-generic) Provider combinator, exactly like Cache in
// the spine — `inner: Provider`, plus the step log and this step's coordinate.
// It claims `Provider` only: streaming/realtime/tools do NOT compose with
// at-least-once replay (you cannot replay half a token stream from a step log),
// so we deliberately do NOT forward those capabilities. See evaluation (c).
class Durable {
  inner: Provider
  log: StepLog
  coord: StepCoord

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // 1. Replay path: if the engine already recorded this coordinate, return
      //    the recorded response. We NEVER re-sample the model on replay — this
      //    is the whole correctness property.
      let prior: string? = self.log.recorded(self.coord) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "step log read failed" }; }
      };
      match (prior) {
        let hit: string => {
          // Determinism guard: the prompt at this coordinate must match what we
          // recorded against, or the workflow body was edited non-deterministically
          // between runs (a §4 determinism violation). The engine stores the
          // prompt fingerprint alongside the value; mismatch is a hard error, not
          // a silent re-sample.
          let fp: string = baml.llm.prompt_fingerprint(prompt);
          if (!durable_envelope_matches(hit, fp)) {
            throw baml.errors.LlmClient {
              message: "non-determinism at step '" + self.coord.step
                       + "': prompt changed across replay"
            };
          }
          // Decode the recorded wire response back into T with the same parser
          // path the live call would have used. Recording is at the response
          // layer, not the typed layer, so a schema change between runs still
          // re-parses the SAME bytes (no re-sample).
          return durable_decode<T>(durable_envelope_body(hit));
        }
        null => {
          // 2. First-execution path: sample exactly once, then record. If the
          //    process crashes AFTER record() but before the engine commits the
          //    step, the engine's at-least-once envelope re-enters here, finds
          //    the record on replay, and takes the replay path above. If it
          //    crashes BEFORE record(), we re-sample — that is the at-least-once
          //    window the author must accept for a model call (it has no external
          //    side effect, so re-sampling once is safe; tool side effects are
          //    handled separately, below).
          let answer: T = self.inner.call<T>(prompt);
          let fp: string = baml.llm.prompt_fingerprint(prompt);
          let envelope: string = durable_envelope_new(fp, durable_encode<T>(answer));
          self.log.record(self.coord, envelope) catch (e) {
            _ => { throw baml.errors.LlmClient { message: "step log write failed" }; }
          };
          return answer;
        }
      }
    }
  }
}

// Combinator-construction sugar on Provider, mirroring `.cached(store)` in the
// spine. `p.durable(log, coord)` reads like the other combinators.
interface DurableExt requires Provider {
  function durable(self, log: StepLog, coord: StepCoord) -> Durable {
    Durable { inner: self, log: log, coord: coord }
  }
}

// ── Record/replay envelope helpers (pure host functions). ────────────────────
// The envelope is {prompt_fingerprint, body}: the fingerprint powers the
// determinism guard, the body is the recorded wire response we re-parse.
function durable_envelope_new(fingerprint: string, body: string) -> string { $rust_io_function }
function durable_envelope_matches(envelope: string, fingerprint: string) -> bool { $rust_io_function }
function durable_envelope_body(envelope: string) -> string { $rust_io_function }
function durable_encode<T>(value: T) -> string { $rust_io_function }   // T -> recordable string
function durable_decode<T>(body: string) -> T { $rust_io_function }    // recorded string -> T

// `prompt_fingerprint` is net-new on baml.llm: a stable hash of the rendered
// PromptAst so we can detect a body edit across replay. PromptAst is opaque, so
// only the host can fingerprint it.
//   namespace baml.llm { function prompt_fingerprint(p: PromptAst) -> string }

// ─────────────────────────────────────────────────────────────────────────────
// Engine adapters: Temporal / DBOS / Inngest. Each is a StepLog backed by the
// engine's own step-log primitive. The model is identical; only the host call
// differs (background §4 "What varies": external server vs Postgres log vs
// event-sourced memoization).
// ─────────────────────────────────────────────────────────────────────────────

class TemporalStepLog {
  // The Temporal SDK threads the workflow context (run id, command seq) through
  // a $rust_type handle. recorded/record become activity-result lookups on the
  // event-sourced history.
  _ctx: $rust_type
  implements StepLog {
    function recorded(self, coord: StepCoord) -> string? throws baml.errors.Io { $rust_io_function }
    function record(self, coord: StepCoord, value: string) -> null throws baml.errors.Io { $rust_io_function }
    function deterministic_now(self) -> string throws baml.errors.Io { $rust_io_function }
  }
}

class DbosStepLog {
  // DBOS: a Postgres-backed step log, in-process, no separate server. recorded =
  // SELECT on (run, step); record = INSERT ... ON CONFLICT DO NOTHING (the
  // dedupe that makes record idempotent under at-least-once).
  _conn: $rust_type
  implements StepLog {
    function recorded(self, coord: StepCoord) -> string? throws baml.errors.Io { $rust_io_function }
    function record(self, coord: StepCoord, value: string) -> null throws baml.errors.Io { $rust_io_function }
    function deterministic_now(self) -> string throws baml.errors.Io { $rust_io_function }
  }
}

class InngestStepLog {
  // Inngest: event-driven step memoization. recorded = the memoized result for
  // this step id within the function run; record = the step's return value the
  // engine checkpoints.
  _step: $rust_type
  implements StepLog {
    function recorded(self, coord: StepCoord) -> string? throws baml.errors.Io { $rust_io_function }
    function record(self, coord: StepCoord, value: string) -> null throws baml.errors.Io { $rust_io_function }
    function deterministic_now(self) -> string throws baml.errors.Io { $rust_io_function }
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Idempotent tool dispatch. The model call is the EASY half — it has no external
// side effect, so re-sampling once in the crash window is harmless once we
// record. The HARD half (background §4 "Idempotency is the author's burden") is
// a tool that charges a card. At-least-once means dispatch may re-run after a
// crash; the engine cannot make the EFFECT once-only. We give the author the
// idempotency key (derived from run + the tool call's opaque id) and the step
// log to dedupe against — but the once-only write is still their code.
//
// This wraps ExecutionContext.dispatch (the Tools seam, §7) so each tool call
// runs as its own checkpointed step. We do NOT make Tools itself durable (the
// transcript is provider-owned + opaque and can't be replayed through a step
// log); we make the dispatch SIDE of the loop durable, which is where the
// dangerous side effects live.
class DurableDispatch {
  inner: ExecutionContext               // the real tool router
  log: StepLog
  run: string

  function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.Io {
    let out: ToolResult[] = [];
    for (let c in calls) {
      // Coordinate per tool call: the call's opaque id IS the idempotency key,
      // tied to the run. Re-dispatch on replay hits the same coordinate.
      let coord: StepCoord = StepCoord { run: self.run, step: "tool:" + c.id };
      let prior: string? = self.log.recorded(coord) catch (e) {
        _ => { throw baml.errors.Io { message: "tool step log read failed" }; }
      };
      match (prior) {
        let hit: string => {
          // Already executed in a prior attempt: replay its recorded output,
          // do NOT re-invoke the tool (do NOT charge the card twice).
          out = out + [ToolResult { id: c.id, output: durable_decode<unknown>(hit) }];
        }
        null => {
          // First execution of this tool call. The author's tool body should
          // ALSO carry an idempotency key (passed via the call args) for the
          // belt-and-suspenders case where we crash after the side effect but
          // before record() — the engine re-enters, recorded() is still null,
          // and only the downstream API's dedupe prevents a double charge.
          let results: ToolResult[] = self.inner.dispatch([c]);
          let r: ToolResult = single(results);
          self.log.record(coord, durable_encode<unknown>(r.output)) catch (e) {
            _ => { throw baml.errors.Io { message: "tool step log write failed" }; }
          };
          out = out + [r];
        }
      }
    }
    out
  }
}

function single(xs: ToolResult[]) -> ToolResult { $rust_io_function }   // exactly-one helper
