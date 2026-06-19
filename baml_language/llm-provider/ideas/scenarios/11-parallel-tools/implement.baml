// =============================================================================
// 11 — Parallel tool calls  ::  implement.baml  (LIBRARY / STD-LIB author code)
// =============================================================================
//
// Builds on the canonical primitives in ../_conventions.md VERBATIM:
//   interface Provider          — call<T>
//   interface Tools             — begin / step / submit / default run_tools
//   class ToolCall  { id, name, args }      // id OPAQUE to the loop
//   class ToolResult{ id, output }
//   class ToolCalls { calls }               // the Iterator "Done" sentinel
//   class Tool      { name, description, parameters: type }
//   class ExecutionContext { ... dispatch(calls) -> ToolResult[] ... }
//
// THE TEACHING POINT of THIS file: when the model emits SEVERAL tool calls in one
// assistant turn, "run them concurrently and feed all results back together" is
// NOT a provider concern. The provider already handed the loop a `ToolCalls` with
// N calls; how those N are executed (sequential, fan-out, gated, semaphore-
// limited, stop-on-first-error) is purely the implementation of ONE method:
//
//     ExecutionContext.dispatch(calls: ToolCall[]) -> ToolResult[]
//
// which sits ABOVE the ToolCall/ToolResult seam. So the entire scenario is a set
// of `dispatch` strategies, and the provider's begin/step/submit (the wire
// threading: id-vs-name correlation, Anthropic positional ordering, cache-stable
// re-send) are REUSED UNCHANGED from scenario 09. We restate two provider tool
// loops only to PROVE that — see how little of them this file actually touches.
//
// What is genuinely NET-NEW here (flagged in evaluation.md):
//   (1) `dispatch` becomes a strategy object — a `Dispatcher` interface, not a
//       single fixed method body. ExecutionContext.dispatch DELEGATES to it.
//   (2) A `@side_effect` / read-only annotation on `Tool`, because there is NO
//       standard wire metadata that marks a tool safe to parallelize. We add a
//       `effect: ToolEffect` field to `Tool`.
//   (3) A host concurrency primitive: `baml.async.gather` (run-all, order-
//       preserving) and `baml.async.gather_until_error` (stop-on-first-error
//       with cooperative cancellation), plus a `Semaphore` for rate-limit caps.
// =============================================================================


// -----------------------------------------------------------------------------
// NET-NEW #2: tool effect metadata.
//
// The background's hardest point: "there is no standard tool metadata that marks
// a tool read-only", so frameworks cannot auto-decide what is safe to fan out.
// We make the missing metadata first-class. `effect` defaults to `write` — the
// SAFE default (assume a tool has side effects unless told otherwise), so adding
// a tool without thinking never silently parallelizes a `charge_card`.
//
// NOTE: this EXTENDS the canonical `Tool` (../_conventions.md). It is additive —
// every provider's encoder ignores `effect` (it never goes on the wire); only the
// dispatcher reads it. That separation is the whole reason it can live here.
// -----------------------------------------------------------------------------
enum ToolEffect {
  ReadOnly      // pure / idempotent: safe to fan out concurrently (search, lookup)
  Write         // mutates external state: must be sequenced (charge_card, send_email)
}

class Tool {
  name: string
  description: string
  parameters: type
  effect: ToolEffect = Write     // SAFE default: a tool is "write" unless declared read-only
}


// -----------------------------------------------------------------------------
// NET-NEW #3: host concurrency primitives.
//
// BAML has no concurrency surface today. Parallel dispatch needs three host
// boundaries. All three are deterministic in their OUTPUT ORDER regardless of
// completion order — the determinism the background demands for Anthropic's
// positional matching and for prompt-cache stability.
// -----------------------------------------------------------------------------

// Run all of `tasks` concurrently; collect results in the SAME index order as the
// input, after every task settles. A task that throws is NOT propagated — it is
// captured as a `Settled.failure` so siblings still finish (run-all policy). This
// is `asyncio.gather(..., return_exceptions=True)` / `Promise.allSettled`.
class Settled<T> {
  ok: T?
  failure: string?         // present iff the task threw; message only
}
function gather<T>(tasks: (() -> T)[]) -> Settled<T>[] { $rust_io_function }

// Run all concurrently, but the FIRST task to throw cancels the rest
// (cooperative cancellation) and the whole call throws. This is
// `asyncio.gather(...)` default / `Promise.all`. Order-preserving on success.
function gather_until_error<T>(tasks: (() -> T)[]) -> T[] throws baml.errors.LlmClient { $rust_io_function }

// A counting semaphore for tool-level rate limits. `run` blocks until a permit is
// free, runs the thunk, releases. Host-backed because it needs real async waiting.
class Semaphore {
  permits: int
  _handle: $rust_type
  function acquire_run<T>(self, thunk: () -> T) -> T throws baml.errors.LlmClient { $rust_io_function }
}
function semaphore(permits: int) -> Semaphore { $rust_io_function }


// -----------------------------------------------------------------------------
// NET-NEW #1: dispatch becomes a STRATEGY, not a fixed method body.
//
// In scenario 09, `ExecutionContext.dispatch` was a fixed sequential for-loop.
// To make parallelism / gating / limits pluggable WITHOUT touching the provider,
// we lift the body into a `Dispatcher` interface. ExecutionContext holds one and
// forwards to it. The provider's `run_tools` default still calls
// `ctx.dispatch(calls)` verbatim — it has no idea which strategy ran.
//
// A Dispatcher's ONE method is the parallel/sequential seam. Everything below
// `invoke_one` (handler lookup, arg validation against `parameters: type`, deps
// injection, id stamping) is shared and identical to scenario 09.
// -----------------------------------------------------------------------------
interface Dispatcher {
  // Given the calls the model emitted in ONE turn, produce their results.
  // MUST return results stamped with the SAME ToolCall.id (so the provider can
  // correlate / re-order positionally below the seam). The ORDER of the returned
  // array is irrelevant to correctness — providers re-key by id or re-order by
  // their recorded tool_use order (Anthropic). But a well-behaved dispatcher
  // returns them in INPUT order anyway, for cache stability when the provider is
  // id-agnostic about order (OpenAI keeps whatever order it receives).
  function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient
}

// The handler registry + deps. This is the shared "below invoke_one" machinery
// every dispatcher reuses; it is NOT itself a strategy. `invoke_one` is the leaf
// that actually runs one tool and stamps the id — the unit of work a strategy
// schedules. It is a `() -> ToolResult` THUNK so a strategy can hand it to
// `gather` / a semaphore without running it eagerly.
class Registry {
  deps: unknown                 // injected out-of-band state (DB, user, auth) — model never sees it
  _handlers: $rust_type         // name -> handler closure, registered host-side

  // Resolve handler by name, SAP-validate `c.args` against the tool's `type`,
  // inject deps, invoke, stamp `c.id`. A failure is thrown by THIS — the strategy
  // decides whether to swallow it (run-all) or propagate (stop-on-first-error).
  function invoke_one(self, c: ToolCall) -> ToolResult throws baml.errors.LlmClient {
    $rust_io_function
  }

  // The thunk a strategy schedules. Capturing `c` and `self` lets `gather` run
  // many of these concurrently.
  function thunk_for(self, c: ToolCall) -> (() -> ToolResult) {
    () => self.invoke_one(c)
  }

  // Look up a call's declared effect from the tool table (read-only vs write).
  // Returns Write if the tool is unknown — fail safe toward sequencing.
  function effect_of(self, c: ToolCall) -> ToolEffect { $rust_io_function }
}


// -----------------------------------------------------------------------------
// STRATEGY A — Sequential (the scenario-09 baseline, restated as a Dispatcher).
//
// total time = sum of all calls. The reference for "what parallelism speeds up".
// Run-all: a failure becomes an error ToolResult, siblings still run.
// -----------------------------------------------------------------------------
class SequentialDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient {
      let out: ToolResult[] = [];
      for (let c in calls) {
        let r: ToolResult = reg.invoke_one(c) catch (e) {
          _ => { ToolResult { id: c.id, output: { "error": "tool execution failed" } } }
        };
        out = out.append(r);
      }
      out
    }
  }
}


// -----------------------------------------------------------------------------
// STRATEGY B — Parallel, run-all, order-preserving (the headline of the scenario).
//
// "Start all calls at once, append results in the ORIGINAL order." Implemented as
// collect-then-append: build a thunk per call, `gather` them concurrently (which
// settles ALL of them, capturing per-call failures), then walk the settled array
// IN INPUT ORDER and project each to a ToolResult. A failed call becomes an error
// ToolResult — the model sees everything and decides (run-all). Determinism +
// Anthropic positional matching + cache stability all fall out of "the writer
// emits in call order", NOT completion order.
// -----------------------------------------------------------------------------
class ParallelDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient {
      // 1. build one thunk per call (does NOT run yet).
      let thunks: (() -> ToolResult)[] = [];
      for (let c in calls) {
        thunks = thunks.append(reg.thunk_for(c));
      }
      // 2. fan out. gather settles every task and preserves index order.
      let settled: Settled<ToolResult>[] = gather<ToolResult>(thunks);
      // 3. SINGLE writer, INPUT ORDER: project settled -> ToolResult. A failure
      //    becomes an error result stamped with the original call's id (run-all).
      let out: ToolResult[] = [];
      let i: int = 0;
      for (let s in settled) {
        let original: ToolCall = calls.at(i);
        match (s.ok) {
          let r: ToolResult => { out = out.append(r); },
          _ => {
            // s.failure is set; surface it as a result the model can react to.
            out = out.append(ToolResult { id: original.id, output: { "error": s.failure } });
          },
        }
        i = i + 1;
      }
      out
    }
  }
}


// -----------------------------------------------------------------------------
// STRATEGY C — Parallel, stop-on-first-error.
//
// The other partial-failure policy: the first call to throw cancels the in-flight
// siblings and aborts the turn. Needs cooperative cancellation, which lives in the
// host primitive `gather_until_error`. NOTE the visible cost: because we abort, we
// CANNOT return a result per call — so this strategy THROWS rather than feeding a
// half-turn back. That asymmetry (run-all returns; stop-on-error throws) is real
// and is called out in the evaluation.
// -----------------------------------------------------------------------------
class StopOnFirstErrorDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient {
      let thunks: (() -> ToolResult)[] = [];
      for (let c in calls) {
        thunks = thunks.append(reg.thunk_for(c));
      }
      // throws on the FIRST failure (siblings cancelled). On success, order-preserved.
      gather_until_error<ToolResult>(thunks)
    }
  }
}


// -----------------------------------------------------------------------------
// STRATEGY D — Effect-aware: fan out the read-only calls, SEQUENCE the writes.
//
// The background's central safety constraint: "pure read-only tools are safe to
// parallelize; side-effecting tools usually want sequencing." With no wire
// metadata, we use the `Tool.effect` annotation we added. Within ONE turn:
//   * partition calls by declared effect;
//   * fan out ReadOnly concurrently (run-all);
//   * run Write calls SEQUENTIALLY, in their original relative order;
//   * re-assemble the full result list in ORIGINAL call order (positional/cache).
//
// This is the closest a dispatcher can get to "charge only if email succeeded"
// WITHOUT cross-call transactionality — and the evaluation is honest that it is
// only sequencing, not a real gate (see "side effects under concurrency" there).
// -----------------------------------------------------------------------------
class EffectAwareDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient {
      // 1. partition while remembering each call's ORIGINAL index for re-assembly.
      let read_thunks: (() -> ToolResult)[] = [];
      let read_index: int[] = [];
      let write_calls: ToolCall[] = [];
      let write_index: int[] = [];
      let i: int = 0;
      for (let c in calls) {
        match (reg.effect_of(c)) {
          ToolEffect::ReadOnly => {
            read_thunks = read_thunks.append(reg.thunk_for(c));
            read_index = read_index.append(i);
          },
          ToolEffect::Write => {
            write_calls = write_calls.append(c);
            write_index = write_index.append(i);
          },
        }
        i = i + 1;
      }

      // 2. fan out the reads (run-all), sequence the writes in relative order.
      let read_settled: Settled<ToolResult>[] = gather<ToolResult>(read_thunks);
      let write_results: ToolResult[] = [];
      for (let wc in write_calls) {
        let r: ToolResult = reg.invoke_one(wc) catch (e) {
          _ => { ToolResult { id: wc.id, output: { "error": "tool execution failed" } } }
        };
        write_results = write_results.append(r);
      }

      // 3. SCATTER back into a slot array indexed by original position, so the
      //    final order is the model's emission order — positional & cache stable.
      let slots: ToolResult?[] = ParallelHelpers.empty_slots(calls.length());
      let ri: int = 0;
      for (let s in read_settled) {
        let orig: int = read_index.at(ri);
        let oc: ToolCall = calls.at(orig);
        match (s.ok) {
          let r: ToolResult => { slots = slots.set(orig, r); },
          _ => { slots = slots.set(orig, ToolResult { id: oc.id, output: { "error": s.failure } }); },
        }
        ri = ri + 1;
      }
      let wi: int = 0;
      for (let r in write_results) {
        slots = slots.set(write_index.at(wi), r);
        wi = wi + 1;
      }
      ParallelHelpers.unwrap_slots(slots)        // assert all filled, drop the option
    }
  }
}

class ParallelHelpers {
  function empty_slots(n: int) -> ToolResult?[] { $rust_io_function }
  function unwrap_slots(slots: ToolResult?[]) -> ToolResult[] { $rust_io_function }
}


// -----------------------------------------------------------------------------
// STRATEGY E — Concurrency-limited (a decorator over ANOTHER strategy).
//
// "Three concurrent calls to the same API can blow a rate limit." A semaphore is
// the caller's problem — and it is ORTHOGONAL to run-all-vs-stop-on-error. So we
// make it a DECORATOR: it wraps each thunk in `semaphore.acquire_run`, then hands
// the throttled thunks to its inner strategy. Composition over inheritance.
//
// To throttle, the limiter has to wrap thunks BEFORE the inner strategy schedules
// them. So it doesn't delegate to `inner.dispatch` directly; it owns the same
// fan-out shape and reuses run-all semantics. (We keep it run-all here; a stop-
// on-error limited variant would compose `gather_until_error` instead.)
// -----------------------------------------------------------------------------
class LimitedDispatch {
  limit: int

  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.errors.LlmClient {
      let sem: Semaphore = semaphore(self.limit);
      // wrap each unit of work so it must hold a permit to run.
      let throttled: (() -> ToolResult)[] = [];
      for (let c in calls) {
        let base: (() -> ToolResult) = reg.thunk_for(c);
        throttled = throttled.append(() => sem.acquire_run<ToolResult>(base));
      }
      let settled: Settled<ToolResult>[] = gather<ToolResult>(throttled);
      let out: ToolResult[] = [];
      let i: int = 0;
      for (let s in settled) {
        let oc: ToolCall = calls.at(i);
        match (s.ok) {
          let r: ToolResult => { out = out.append(r); },
          _ => { out = out.append(ToolResult { id: oc.id, output: { "error": s.failure } }); },
        }
        i = i + 1;
      }
      out
    }
  }
}


// -----------------------------------------------------------------------------
// ExecutionContext: holds the registry + the chosen Dispatcher, and forwards.
//
// This is the ONE seam that scenario 09's `run_tools` default already calls
// (`ctx.dispatch(calls.calls)`). By making `dispatch` forward to a strategy, the
// ENTIRE parallel-tools scenario plugs in WITHOUT a single change to begin/step/
// submit or to run_tools. That is the proof the model "absorbs" parallelism.
// -----------------------------------------------------------------------------
class ExecutionContext {
  function_name: string
  args: map<string, unknown>
  registry: Registry
  dispatcher: Dispatcher       // strategy object — sequential / parallel / effect-aware / limited

  function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.errors.LlmClient {
    self.dispatcher.dispatch(calls, self.registry)
  }
}


// =============================================================================
// The two providers below are RESTATED FROM SCENARIO 09, abbreviated, ONLY to
// prove the point: NOTHING in their begin/step/submit changes for parallelism.
// The id-vs-name correlation (Anthropic id-keyed + positional re-order; Gemini
// no-id synthesis) is exactly as before. Parallelism happens entirely between
// `step` returning ToolCalls and `submit` receiving ToolResult[], inside the
// dispatcher the loop calls. The providers are SPECTATORS to the strategy.
// =============================================================================

// PROVIDER 1 — Anthropic. The one parallelism stresses MOST, because Anthropic
// processes tool_result blocks POSITIONALLY: the results submit() threads back
// must be in the SAME order as the tool_use blocks. The dispatcher returns
// results in input order; even if it didn't, submit() re-orders by t.pending_ids.
// So positional correctness is defended at BOTH layers — belt and suspenders.
class Anthropic {
  model: string
  api_key: string?
  max_tokens: int                        // bespoke required field — a provider field, no options blob

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Tools {
    type Transcript = AnthropicTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // tools encoded to input_schema; `effect` is NOT serialized (it never goes
      // on the wire — only the dispatcher reads it).
      AnthropicTranscript {
        model: self.model,
        api_key: self.api_key,
        max_tokens: self.max_tokens,
        messages: baml.llm.prompt_to_anthropic_messages(prompt),
        wire_tools: Anthropic.encode_tools_anthropic(tools),
        pending_ids: [],
      }
    }

    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      let resp = Anthropic.post_messages(t) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "anthropic messages failed" }; }
      };
      let body: string = resp.text();
      if (baml.llm.anthropic_stop_reason(body) == "tool_use") {
        // MULTIPLE tool_use blocks in one turn = the parallel case. We surface them
        // ALL as one ToolCalls; the dispatcher decides how to run them.
        let raw: AnthropicToolUse[] = baml.llm.anthropic_extract_tool_use(body);
        let calls: ToolCall[] = [];
        let ids: string[] = [];
        for (let r in raw) {
          calls = calls.append(ToolCall { id: r.id, name: r.name, args: r.input });
          ids = ids.append(r.id);                 // positional order recorded for submit()
        }
        Anthropic.remember_assistant_turn(t, body, ids);
        return ToolCalls { calls: calls };
      }
      baml.sap.parse<T>(baml.llm.anthropic_text(body))
    }

    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.errors.LlmClient {
      // POSITIONAL DISCIPLINE: re-sort by the recorded tool_use order before
      // serializing. The dispatcher SHOULD already return input order, but this
      // guarantees Anthropic's positional matching even if a future strategy
      // returns completion order. This is the cache-stability defense too.
      let ordered: ToolResult[] = Anthropic.reorder_by_ids(results, t.pending_ids);
      let blocks: string[] = [];
      for (let r in ordered) {
        blocks = blocks.append(
          baml.llm.anthropic_tool_result_block(r.id, baml.sap.to_json(r.output))
        );
      }
      Anthropic.append_user_message(t, blocks)
    }
  }

  function encode_tools_anthropic(tools: Tool[]) -> string throws baml.errors.LlmClient { $rust_io_function }
  function reorder_by_ids(results: ToolResult[], order: string[]) -> ToolResult[] {
    let out: ToolResult[] = [];
    for (let id in order) {
      for (let r in results) {
        if (r.id == id) { out = out.append(r); }
      }
    }
    out
  }
  function post_messages(t: AnthropicTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_assistant_turn(t: AnthropicTranscript, body: string, ids: string[]) -> null { $rust_io_function }
  function append_user_message(t: AnthropicTranscript, blocks: string[]) -> AnthropicTranscript { $rust_io_function }
}

class AnthropicTranscript {
  model: string
  api_key: string?
  max_tokens: int
  messages: string
  wire_tools: string
  pending_ids: string[]                  // tool_use id order — the positional re-assembly key
}
class AnthropicToolUse { id: string, name: string, input: map<string, unknown> }


// PROVIDER 2 — Gemini. No call ids; correlation by function NAME. Same-function
// fan-out (two `search_web` calls in one turn) is the case name-keying CANNOT
// disambiguate. The provider already solved it in scenario 09 by synthesizing a
// stable id per (name, position) below the seam. Parallel dispatch makes that
// MORE important, not different: the dispatcher fans out by the synthetic id, and
// submit() resolves synthetic ids back to (name, position) for the wire.
class Gemini {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements Tools {
    type Transcript = GeminiTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> GeminiTranscript throws baml.errors.LlmClient {
      GeminiTranscript {
        model: self.model,
        api_key: self.api_key,
        contents: baml.llm.prompt_to_gemini_contents(prompt),
        wire_tools: Gemini.encode_tools_gemini(tools),
        id_map: [],
      }
    }

    function step<T>(self, t: GeminiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      let resp = Gemini.post_generate(t) catch (e) {
        _ => { throw baml.errors.LlmClient { message: "gemini generateContent failed" }; }
      };
      let body: string = resp.text();
      let raw: GeminiFunctionCall[] = baml.llm.gemini_extract_function_calls(body);
      if (raw.length() > 0) {
        // SYNTHESIZE ids: name+position disambiguates two concurrent same-function
        // calls — the exact case parallelism makes common. The dispatcher then
        // treats these like any id-keyed calls.
        let calls: ToolCall[] = [];
        let mapping: GeminiIdEntry[] = [];
        let index: int = 0;
        for (let r in raw) {
          let synthetic: string = Gemini.synth_id(r.name, index);
          calls = calls.append(ToolCall { id: synthetic, name: r.name, args: r.args });
          mapping = mapping.append(GeminiIdEntry { id: synthetic, name: r.name, index: index });
          index = index + 1;
        }
        Gemini.remember_model_turn(t, body, mapping);
        return ToolCalls { calls: calls };
      }
      baml.sap.parse<T>(baml.llm.gemini_text(body))
    }

    function submit(self, t: GeminiTranscript, results: ToolResult[]) -> GeminiTranscript throws baml.errors.LlmClient {
      // resolve synthetic ids -> (name, index); emit functionResponse parts in
      // recorded index order so positional correlation holds.
      let parts: string[] = [];
      for (let entry in Gemini.sorted_by_index(t.id_map)) {
        for (let r in results) {
          if (r.id == entry.id) {
            parts = parts.append(
              baml.llm.gemini_function_response(entry.name, baml.sap.to_json(r.output))
            );
          }
        }
      }
      Gemini.append_function_turn(t, parts)
    }
  }

  function encode_tools_gemini(tools: Tool[]) -> string throws baml.errors.LlmClient { $rust_io_function }
  function synth_id(name: string, index: int) -> string { $rust_io_function }
  function sorted_by_index(m: GeminiIdEntry[]) -> GeminiIdEntry[] { $rust_io_function }
  function post_generate(t: GeminiTranscript) -> baml.http.Response throws baml.errors.LlmClient { $rust_io_function }
  function remember_model_turn(t: GeminiTranscript, body: string, m: GeminiIdEntry[]) -> null { $rust_io_function }
  function append_function_turn(t: GeminiTranscript, parts: string[]) -> GeminiTranscript { $rust_io_function }
}

class GeminiTranscript {
  model: string
  api_key: string
  contents: string
  wire_tools: string
  id_map: GeminiIdEntry[]
}
class GeminiFunctionCall { name: string, args: map<string, unknown> }
class GeminiIdEntry { id: string, name: string, index: int }
