// scenario 11 — parallel tools: ../../../../../llm-provider/ideas/scenarios/11-parallel-tools/implementation.baml
//   implementation.baml — LIBRARY / std-lib-author code (the dispatch strategies).
//
// The design's teaching point: when the model emits SEVERAL tool calls in ONE
// assistant turn, "run them concurrently and feed all results back together" is
// NOT a provider concern. The provider's begin/step/submit (id correlation,
// Anthropic positional ordering, cache-stable re-send) are REUSED UNCHANGED from
// scenario 09; parallelism lives entirely in the dispatch step, ABOVE the
// ToolCall/ToolResult seam.
//
// On the REAL surface `run_tools`/`ToolLoop` take dispatch as a plain closure
//     (ToolCall[]) -> ToolResult[] throws never
// (not the design's `Dispatcher` interface / `ExecutionContext`). So each design
// "strategy" becomes a FACTORY that closes over the per-call unit of work
// (`invoke_one`) and returns that dispatch closure. This is design-true: the
// closure is exactly where parallelism lives.
//
// CONCURRENCY IS NOT NET-NEW: structured concurrency (`spawn`/`await`/`Future`,
// `baml.future.all`, `baml.spawn.TaskGroup`) is a SHIPPED first-class language
// feature (BEP-034). Fan-out is a list of spawns, the join is `baml.future.all`
// (order-preserving output — the determinism Anthropic positional matching and
// prompt-cache stability need), and the rate-limit cap is a `TaskGroup`.
//
// Divergences from the design (all forced by the real surface, see 11.json):
//   * `Tool.effect` field  → the stdlib `baml.ai.Tool` cannot be extended here, so
//     effect metadata rides ALONGSIDE the tools as a read-only NAME set + the
//     `s11_ToolEffect` classifier. Same behavior, different carrier.
//   * `Dispatcher` interface + `ExecutionContext` → the real dispatch is a plain
//     closure, so strategies are closure FACTORIES (below). No interface needed.


// The per-call unit of work the strategies schedule is a THUNK-per-call closure
//     (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError
// (`invoke_one`): coerce args (canonically via `baml.ai.invoke_tool<A>`), inject
// deps by CAPTURE, invoke, stamp the id. It MAY throw — a genuine tool failure
// (backend down) is a throw the run-all strategies catch into an error ToolResult
// the model can react to.


// ── run-all per-task union ───────────────────────────────────────────────────
// The value a spawn's catch arm produces so the Future is infallible and
// `baml.future.all` never throws — a failed sibling becomes a captured failure,
// siblings keep running (Promise.allSettled semantics without a new combinator).
class s11_Settled {
    ok: baml.ai.ToolResult?,
    failure: string?,
}


// ── effect metadata (the design's net-new `Tool.effect`, re-homed) ───────────
enum s11_ToolEffect {
    ReadOnly,   // pure / idempotent: safe to fan out (search, lookup)
    Write,      // mutates external state: must be sequenced (charge_card, send_email)
}

/// Classify a call by whether its tool name is in the declared read-only set.
/// Unknown ⇒ Write — fail safe toward sequencing (never silently parallelize a charge).
function s11_effect_of(read_only: string[], c: baml.ai.ToolCall) -> s11_ToolEffect throws never {
    if (read_only.includes(c.name)) {
        s11_ToolEffect.ReadOnly
    } else {
        s11_ToolEffect.Write
    }
}


// ── STRATEGY A — Sequential (scenario-09 baseline; the "what parallelism speeds
// up" reference). Run-all: a failure becomes an error ToolResult, siblings run. ─
function s11_sequential_dispatch(
    invoke_one: (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError,
) -> ((baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never) {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        let out: baml.ai.ToolResult[] = [];
        for (let c in calls) {
            let r: baml.ai.ToolResult = invoke_one(c) catch (e) {
                _ => baml.ai.ToolResult { id: c.id, output: "{\"error\": \"tool execution failed\"}" },
            };
            out.push(r);
        }
        out
    }
}


// ── STRATEGY B — Parallel, run-all, order-preserving (the HEADLINE) ──────────
// Start every call at once; append results in ORIGINAL order. Each spawn CATCHES
// its own failure inside the body (so the Future is infallible), `baml.future.all`
// joins in INPUT order, then a SINGLE writer walks the settled array in input
// order projecting each to a ToolResult (a failure → an error result stamped with
// the original id). Determinism / positional matching / cache stability all fall
// out of "the writer emits in call order", never completion order.
function s11_parallel_dispatch(
    invoke_one: (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError,
) -> ((baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never) {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        let fs: baml.future.Future<s11_Settled, null>[] = calls.map((c) -> {
            spawn {
                s11_Settled { ok: invoke_one(c), failure: null } catch (e) {
                    _ => s11_Settled { ok: null, failure: "tool execution failed" },
                }
            }
        });
        let settled: s11_Settled[] = await baml.future.all(fs);
        s11_project_settled(settled, calls)
    }
}

/// Shared projection: settled (in input order) -> ToolResult[] (input order),
/// a failure becoming an error result stamped with the original call's id.
function s11_project_settled(settled: s11_Settled[], calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    let out: baml.ai.ToolResult[] = [];
    let i = 0;
    for (let s in settled) {
        match (s.ok) {
            null => {
                out.push(baml.ai.ToolResult { id: calls[i].id, output: "{\"error\": \"" + (s.failure ?? "tool failed") + "\"}" });
            },
            let r: baml.ai.ToolResult => {
                out.push(r);
            },
        }
        i = i + 1;
    }
    out
}


// ── STRATEGY C — Parallel, stop-on-first-error ──────────────────────────────
// The other partial-failure policy: the first call to throw CANCELS the in-flight
// siblings and aborts the turn. That IS `baml.future.all`'s contract (Promise.all).
// The design's honest asymmetry: because it aborts it CANNOT return a result per
// call, so it THROWS instead of feeding a half-turn back — which is why it is a
// standalone function, NOT a `throws never` dispatch closure pluggable into
// `ToolLoop`/`run_tools`. `await` re-throws the failing `invoke_one`'s CONCRETE
// error unchanged (no concurrency primitive launders it).
function s11_stop_on_first_error(
    invoke_one: (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError,
    calls: baml.ai.ToolCall[],
) -> baml.ai.ToolResult[] throws baml.errors.ToolError | baml.errors.UnknownError {
    let fs: baml.future.Future<baml.ai.ToolResult, baml.errors.ToolError | baml.errors.UnknownError>[] =
        calls.map((c) -> { spawn { invoke_one(c) } });
    await baml.future.all(fs)
}


// ── STRATEGY D — Effect-aware: fan out reads, SEQUENCE writes ────────────────
// The background's safety constraint: read-only tools are safe to parallelize;
// side-effecting tools want sequencing. Within one turn: partition by effect, fan
// out ReadOnly concurrently (run-all), run Write calls SEQUENTIALLY in relative
// order, then re-assemble in ORIGINAL call order (positional / cache stable).
// Honest limit (design evaluation): this is only SEQUENCING, not a real
// cross-call gate ("charge only if email succeeds" is out of scope at this seam).
function s11_effect_aware_dispatch(
    invoke_one: (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError,
    read_only: string[],
) -> ((baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never) {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        // 1. partition, remembering each call's ORIGINAL index for re-assembly.
        let read_calls: baml.ai.ToolCall[] = [];
        let read_index: int[] = [];
        let write_calls: baml.ai.ToolCall[] = [];
        let write_index: int[] = [];
        let i = 0;
        for (let c in calls) {
            match (s11_effect_of(read_only, c)) {
                s11_ToolEffect.ReadOnly => { read_calls.push(c); read_index.push(i); },
                s11_ToolEffect.Write => { write_calls.push(c); write_index.push(i); },
            }
            i = i + 1;
        }

        // 2. fan out the reads (run-all); sequence the writes in relative order.
        let fs: baml.future.Future<s11_Settled, null>[] = read_calls.map((c) -> {
            spawn {
                s11_Settled { ok: invoke_one(c), failure: null } catch (e) {
                    _ => s11_Settled { ok: null, failure: "read failed" },
                }
            }
        });
        let read_settled: s11_Settled[] = await baml.future.all(fs);
        let write_results: baml.ai.ToolResult[] = [];
        for (let wc in write_calls) {
            let r: baml.ai.ToolResult = invoke_one(wc) catch (e) {
                _ => baml.ai.ToolResult { id: wc.id, output: "{\"error\": \"write failed\"}" },
            };
            write_results.push(r);
        }

        // 3. SCATTER back into a slot array indexed by original position, so the
        //    final order is the model's emission order.
        let slots: baml.ai.ToolResult?[] = [];
        for (let c in calls) {
            slots.push(null);
        }
        let ri = 0;
        for (let s in read_settled) {
            let orig = read_index[ri];
            match (s.ok) {
                null => { slots[orig] = baml.ai.ToolResult { id: calls[orig].id, output: "{\"error\": \"" + (s.failure ?? "read failed") + "\"}" }; },
                let r: baml.ai.ToolResult => { slots[orig] = r; },
            }
            ri = ri + 1;
        }
        let wi = 0;
        for (let r in write_results) {
            slots[write_index[wi]] = r;
            wi = wi + 1;
        }

        // 4. unwrap the slots (every position is filled).
        let out: baml.ai.ToolResult[] = [];
        for (let s in slots) {
            match (s) {
                null => { out.push(baml.ai.ToolResult { id: "unfilled", output: "{\"error\": \"unfilled slot\"}" }); },
                let r: baml.ai.ToolResult => { out.push(r); },
            }
        }
        out
    }
}


// ── STRATEGY E — Concurrency-limited (the rate-limit cap) ────────────────────
// "Three concurrent calls to the same API can blow a rate limit." The cap is the
// caller's problem, ORTHOGONAL to run-all-vs-stop. The SHIPPED rate limiter is
// `baml.spawn.TaskGroup`: spawns sharing a group are capped at `limit`; excess
// queue FIFO and start as earlier ones settle (each `spawn` still returns its
// Future immediately). Joined run-all, so a failed call becomes an error result.
function s11_limited_dispatch(
    invoke_one: (baml.ai.ToolCall) -> baml.ai.ToolResult throws baml.errors.ToolError | baml.errors.UnknownError,
    limit: int,
) -> ((baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never) {
    (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
        let g = baml.spawn.TaskGroup.new(limit);
        let fs: baml.future.Future<s11_Settled, null>[] = calls.map((c) -> {
            spawn with baml.spawn.options(group = g) {
                s11_Settled { ok: invoke_one(c), failure: null } catch (e) {
                    _ => s11_Settled { ok: null, failure: "tool execution failed" },
                }
            }
        });
        let settled: s11_Settled[] = await baml.future.all(fs);
        s11_project_settled(settled, calls)
    }
}


// ── The multi-call-per-turn Tools fixture ────────────────────────────────────
// Scenario 10's `ScriptedTools` emits ONE call per turn, so it cannot exercise
// the parallel case. This fake emits a WHOLE BATCH of tool calls in one assistant
// turn (the exact shape parallelism targets), then answers on the next turn. The
// provider is a SPECTATOR to the dispatch strategy — begin/step/submit are the
// scenario-09/10 shape, untouched.
class s11_MultiState {
    answered: bool,
}

class s11_MultiCallTools {
    /// The batch of tool calls emitted in the FIRST assistant turn.
    batch: baml.ai.ToolCall[],
    /// The final value (SAP-parsed into the caller's `T`) once tools are done.
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = s11_MultiState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> s11_MultiState throws never {
            s11_MultiState { answered: false }
        }
        function step<T>(self, t: s11_MultiState) -> T | baml.ai.ToolCalls throws baml.errors.UnknownError {
            if (!t.answered) {
                return baml.ai.ToolCalls { calls: self.batch };
            }
            baml.sap.parse<T>(self.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["multi-call final parse failed"] },
            }
        }
        function submit(self, t: s11_MultiState, results: baml.ai.ToolResult[]) -> s11_MultiState throws never {
            s11_MultiState { answered: true }
        }
    }
}
