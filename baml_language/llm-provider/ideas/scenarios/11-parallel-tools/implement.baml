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
// N calls; how those N are executed (sequential, fan-out, gated, group-rate-
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
//
// CONCURRENCY IS NOT NET-NEW: structured concurrency (`spawn`/`await`/`Future`,
// `baml.future.all`/`all_complete`, `baml.spawn.TaskGroup`, `CancelToken`) is a
// SHIPPED first-class language feature (BEP-034 — see ../_conventions.md
// "Concurrency", and baml_std/baml/ns_spawn + ns_future). Fan-out is just "a list
// of spawns", the join is `baml.future.all`, and rate-limiting is a `TaskGroup`.
// We use those primitives directly; none of them is a host boundary we invent.
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
// CONCURRENCY: use the SHIPPED BEP-034 primitives directly. No host boundary.
//
// `spawn { body }` starts `body` concurrently and returns a `Future<T,E>`; `await`
// yields the value or re-throws. `baml.future.all` is `Promise.all` (fail-fast,
// cancels losers); `baml.future.all_complete` lets losers finish; the rate limiter
// is `baml.spawn.TaskGroup`. All are order-preserving in their OUTPUT (values come
// back in INPUT order), which is exactly the determinism the background demands for
// Anthropic's positional matching and for prompt-cache stability.
//
// For the RUN-ALL policy we want per-call failure capture (a failed sibling does
// NOT abort the turn — it becomes an error result the model can react to). BEP-034
// has no "allSettled" combinator, but it doesn't need one: each spawn CATCHES its
// own failure and resolves to a `Settled<T>`, so `baml.future.all` over those never
// throws and we get the per-task ok/failure union in input order. `Settled<T>` is a
// plain data record, not a host primitive.
// -----------------------------------------------------------------------------
class Settled<T> {
  ok: T?
  failure: string?         // present iff the spawned body threw; message only
}

// Run every thunk concurrently, capturing per-task failures so siblings always
// finish (run-all / `Promise.allSettled` semantics). Each thunk runs in its own
// `spawn` and catches its own error INSIDE the spawn, so the future is infallible
// and `baml.future.all` returns all of them in INPUT order without ever throwing.
function settle_all<T>(thunks: (() -> T)[]) -> Settled<T>[] {
  let fs: baml.future.Future<Settled<T>, never>[] = thunks.map((thunk) -> {
    spawn {
      // catch INSIDE the spawn body so the Future itself is infallible (`never`):
      // a thrown task becomes a captured `Settled.failure`, siblings keep running.
      // Both arms yield `Settled<T>`, so the spawn body's type is `Settled<T>`.
      Settled<T> { ok: thunk(), failure: null } catch (e) {
        _ => { Settled<T> { ok: null, failure: "tool execution failed" } }
      }
    }
  });
  await baml.future.all(fs)
}


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
  function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError>
}

// The handler registry + deps. This is the shared "below invoke_one" machinery
// every dispatcher reuses; it is NOT itself a strategy. `invoke_one` is the leaf
// that actually runs one tool and stamps the id — the unit of work a strategy
// schedules. It is a `() -> ToolResult` THUNK so a strategy can hand it to
// `settle_all` / a `spawn` without running it eagerly.
class Registry {
  deps: unknown                 // injected out-of-band state (DB, user, auth) — model never sees it
  _handlers: $rust_type         // name -> handler closure, registered host-side

  // Resolve handler by name, SAP-validate `c.args` against the tool's `type`,
  // inject deps, invoke, stamp `c.id`. A failure is thrown by THIS — the strategy
  // decides whether to swallow it (run-all) or propagate (stop-on-first-error).
  function invoke_one(self, c: ToolCall) -> ToolResult throws baml.ExtendUnknownError<baml.errors.ToolError> {
    $rust_io_function
  }

  // The thunk a strategy schedules. Capturing `c` and `self` lets a `spawn` run
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
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let out: ToolResult[] = [];
      for (let c in calls) {
        let r: ToolResult = reg.invoke_one(c) catch (e) {
          _ => { ToolResult { id: c.id, output: { "error": "tool execution failed" } } }
        };
        out.push(r);
      }
      out
    }
  }
}


// -----------------------------------------------------------------------------
// STRATEGY B — Parallel, run-all, order-preserving (the headline of the scenario).
//
// "Start all calls at once, append results in the ORIGINAL order." Implemented as
// collect-then-append: build a thunk per call, `settle_all` spawns them concurrently
// (each spawn captures its own failure, `baml.future.all` joins them in INPUT
// order), then walk the settled array IN INPUT ORDER and project each to a
// ToolResult. A failed call becomes an error ToolResult — the model sees everything
// and decides (run-all). Determinism + Anthropic positional matching + cache
// stability all fall out of "the writer emits in call order", NOT completion order.
// -----------------------------------------------------------------------------
class ParallelDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // 1. build one thunk per call (does NOT run yet).
      let thunks: (() -> ToolResult)[] = [];
      for (let c in calls) {
        thunks.push(reg.thunk_for(c));
      }
      // 2. fan out. settle_all spawns every task and joins them in index order.
      let settled: Settled<ToolResult>[] = settle_all<ToolResult>(thunks);
      // 3. SINGLE writer, INPUT ORDER: project settled -> ToolResult. A failure
      //    becomes an error result stamped with the original call's id (run-all).
      let out: ToolResult[] = [];
      let i: int = 0;
      for (let s in settled) {
        let original: ToolCall = calls.at(i);
        match (s.ok) {
          let r: ToolResult => { out.push(r); },
          _ => {
            // s.failure is set; surface it as a result the model can react to.
            out.push(ToolResult { id: original.id, output: { "error": s.failure } });
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
// siblings and aborts the turn. This is exactly `baml.future.all`'s contract
// (Promise.all): on the first failure it cancels the losing futures and re-throws.
// NOTE the visible cost: because we abort, we CANNOT return a result per call — so
// this strategy THROWS rather than feeding a half-turn back. That asymmetry
// (run-all returns; stop-on-error throws) is real and is called out in the
// evaluation. The thrown error keeps its concrete `ToolError` type — it is the
// failing invoke_one's error, re-thrown by `await` unchanged.
// -----------------------------------------------------------------------------
class StopOnFirstErrorDispatch {
  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Each call runs in its own spawn; the future carries the ToolError channel.
      let fs: baml.future.Future<ToolResult, baml.ExtendUnknownError<baml.errors.ToolError>>[] =
        calls.map((c) -> { spawn { reg.invoke_one(c) } });
      // `all` joins in INPUT order; the FIRST failure cancels the rest and re-throws
      // the original ToolError. `await` surfaces it on this method's channel.
      await baml.future.all(fs)
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
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // 1. partition while remembering each call's ORIGINAL index for re-assembly.
      let read_thunks: (() -> ToolResult)[] = [];
      let read_index: int[] = [];
      let write_calls: ToolCall[] = [];
      let write_index: int[] = [];
      let i: int = 0;
      for (let c in calls) {
        match (reg.effect_of(c)) {
          ToolEffect::ReadOnly => {
            read_thunks.push(reg.thunk_for(c));
            read_index.push(i);
          },
          ToolEffect::Write => {
            write_calls.push(c);
            write_index.push(i);
          },
        }
        i = i + 1;
      }

      // 2. fan out the reads (run-all), sequence the writes in relative order.
      let read_settled: Settled<ToolResult>[] = settle_all<ToolResult>(read_thunks);
      let write_results: ToolResult[] = [];
      for (let wc in write_calls) {
        let r: ToolResult = reg.invoke_one(wc) catch (e) {
          _ => { ToolResult { id: wc.id, output: { "error": "tool execution failed" } } }
        };
        write_results.push(r);
      }

      // 3. SCATTER back into a slot array indexed by original position, so the
      //    final order is the model's emission order — positional & cache stable.
      let slots: ToolResult?[] = ParallelHelpers.empty_slots(calls.length());
      let ri: int = 0;
      for (let s in read_settled) {
        let orig: int = read_index.at(ri);
        let oc: ToolCall = calls.at(orig);
        match (s.ok) {
          let r: ToolResult => { slots.splice(orig, 1, [r]); },
          _ => { slots.splice(orig, 1, [ToolResult { id: oc.id, output: { "error": s.failure } }]); },
        }
        ri = ri + 1;
      }
      let wi: int = 0;
      for (let r in write_results) {
        slots.splice(write_index.at(wi), 1, [r]);
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
// STRATEGY E — Concurrency-limited (the rate-limit cap).
//
// "Three concurrent calls to the same API can blow a rate limit." The cap is the
// caller's problem — and it is ORTHOGONAL to run-all-vs-stop-on-error. The SHIPPED
// rate limiter is `baml.spawn.TaskGroup`: spawns that share a group are capped at
// `limit`; excess spawns queue FIFO and start as earlier ones settle (each `spawn`
// still returns its Future immediately, so the queueing is invisible). We enroll
// every call's spawn in one group via `spawn with baml.spawn.options(group = g)`,
// then join run-all with `settle_all`'s catch-inside-spawn pattern so a failed call
// becomes an error result instead of aborting the turn.
// -----------------------------------------------------------------------------
class LimitedDispatch {
  limit: int

  implements Dispatcher {
    function dispatch(self, calls: ToolCall[], reg: Registry) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let g = baml.spawn.TaskGroup.new(self.limit);          // cap concurrency at `limit`
      // Spawn every call INTO the group (run-all: each catches its own failure so
      // the future is infallible and `baml.future.all` never throws). At most
      // `limit` run at once; the rest queue FIFO.
      let fs: baml.future.Future<Settled<ToolResult>, never>[] = calls.map((c) -> {
        spawn with baml.spawn.options(group = g) {
          Settled<ToolResult> { ok: reg.invoke_one(c), failure: null } catch (e) {
            _ => { Settled<ToolResult> { ok: null, failure: "tool execution failed" } }
          }
        }
      });
      let settled: Settled<ToolResult>[] = await baml.future.all(fs);
      let out: ToolResult[] = [];
      let i: int = 0;
      for (let s in settled) {
        let oc: ToolCall = calls.at(i);
        match (s.ok) {
          let r: ToolResult => { out.push(r); },
          _ => { out.push(ToolResult { id: oc.id, output: { "error": s.failure } }); },
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

  function dispatch(self, calls: ToolCall[]) -> ToolResult[] throws baml.ExtendUnknownError<baml.errors.ToolError> {
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

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> AnthropicTranscript throws baml.ExtendUnknownError<baml.errors.ToolError> {
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

    function step<T>(self, t: AnthropicTranscript) -> T | ToolCalls throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let resp = Anthropic.post_messages(t);
      let body: string = resp.text();
      if (baml.llm.anthropic_stop_reason(body) == "tool_use") {
        // MULTIPLE tool_use blocks in one turn = the parallel case. We surface them
        // ALL as one ToolCalls; the dispatcher decides how to run them.
        let raw: AnthropicToolUse[] = baml.llm.anthropic_extract_tool_use(body);
        let calls: ToolCall[] = [];
        let ids: string[] = [];
        for (let r in raw) {
          calls.push(ToolCall { id: r.id, name: r.name, args: r.input });
          ids.push(r.id);                 // positional order recorded for submit()
        }
        Anthropic.remember_assistant_turn(t, body, ids);
        return ToolCalls { calls: calls };
      }
      baml.sap.parse<T>(baml.llm.anthropic_text(body))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "anthropic messages failed");
    }

    function submit(self, t: AnthropicTranscript, results: ToolResult[]) -> AnthropicTranscript throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // POSITIONAL DISCIPLINE: re-sort by the recorded tool_use order before
      // serializing. The dispatcher SHOULD already return input order, but this
      // guarantees Anthropic's positional matching even if a future strategy
      // returns completion order. This is the cache-stability defense too.
      let ordered: ToolResult[] = Anthropic.reorder_by_ids(results, t.pending_ids);
      let blocks: string[] = [];
      for (let r in ordered) {
        blocks.push(
          baml.llm.anthropic_tool_result_block(r.id, baml.json.to_string<unknown>(r.output))
        );
      }
      Anthropic.append_user_message(t, blocks)
    }
  }

  function encode_tools_anthropic(tools: Tool[]) -> string throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
  function reorder_by_ids(results: ToolResult[], order: string[]) -> ToolResult[] {
    let out: ToolResult[] = [];
    for (let id in order) {
      for (let r in results) {
        if (r.id == id) { out.push(r); }
      }
    }
    out
  }
  function post_messages(t: AnthropicTranscript) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
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

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> GeminiTranscript throws baml.ExtendUnknownError<baml.errors.ToolError> {
      GeminiTranscript {
        model: self.model,
        api_key: self.api_key,
        contents: baml.llm.prompt_to_gemini_contents(prompt),
        wire_tools: Gemini.encode_tools_gemini(tools),
        id_map: [],
      }
    }

    function step<T>(self, t: GeminiTranscript) -> T | ToolCalls throws baml.ExtendUnknownError<baml.errors.ToolError> {
      let resp = Gemini.post_generate(t);
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
          calls.push(ToolCall { id: synthetic, name: r.name, args: r.args });
          mapping.push(GeminiIdEntry { id: synthetic, name: r.name, index: index });
          index = index + 1;
        }
        Gemini.remember_model_turn(t, body, mapping);
        return ToolCalls { calls: calls };
      }
      baml.sap.parse<T>(baml.llm.gemini_text(body))
    } catch (e) {
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "gemini generateContent failed");
    }

    function submit(self, t: GeminiTranscript, results: ToolResult[]) -> GeminiTranscript throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // resolve synthetic ids -> (name, index); emit functionResponse parts in
      // recorded index order so positional correlation holds.
      let parts: string[] = [];
      for (let entry in Gemini.sorted_by_index(t.id_map)) {
        for (let r in results) {
          if (r.id == entry.id) {
            parts.push(
              baml.llm.gemini_function_response(entry.name, baml.json.to_string<unknown>(r.output))
            );
          }
        }
      }
      Gemini.append_function_turn(t, parts)
    }
  }

  function encode_tools_gemini(tools: Tool[]) -> string throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
  function synth_id(name: string, index: int) -> string { $rust_io_function }
  function sorted_by_index(m: GeminiIdEntry[]) -> GeminiIdEntry[] { $rust_io_function }
  function post_generate(t: GeminiTranscript) -> baml.http.Response throws baml.ExtendUnknownError<baml.errors.ToolError> { $rust_io_function }
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
