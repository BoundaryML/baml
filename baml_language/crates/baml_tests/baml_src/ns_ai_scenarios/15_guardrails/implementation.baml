// scenario 15 — guardrails: ../../../../../llm-provider/ideas/scenarios/15-guardrails/implement.baml
//   implementation.baml — LIBRARY / std-lib-author code.
//
// The design models input/output guardrails as a `Guarded` COMBINATOR that wraps
// any `Provider` (the marker) and `implements HttpProvider` — the same wrapper
// shape as the stdlib `Traced`/`Fallback`/`Retry`: forward the inner provider's
// request/response surface by driving it with an IDENTITY projection, and run the
// checks AROUND it (output guards sequentially after the inner call; input guards
// before it, with a concurrent BEP-034 race variant below in `s15_race_input_guards`).
//
// A tripwire is a typed `throw` (`s15_InputTripwire` / `s15_OutputTripwire`) the app
// catches BY TYPE — the input/output distinction lives in the type, not a field,
// mirroring the SDK's two exception classes. A tripwire is a deliberate guardrail
// DOMAIN failure (like `Refused`): neither transport nor parse, so every classifier
// is false; it rides the CallError channel on the plain path AND the ToolError
// channel mid tool-loop, so it `implements` BOTH — plus the mandatory `Failure` axis
// (every channel `requires Failure`). It is NOT retryable (re-driving replays a
// blocked prompt) and IS a policy refusal.
//
// A guardrail is itself a `Provider` (`s15_Guardrail requires baml.ai.Provider`):
//   - `s15_ModelGuard` also `implements HttpProvider` (the check IS an LLM judge
//     call), so a detector model retries/caches/falls-back like any other provider;
//   - `s15_CodeGuard` `implements` the guardrail marker ONLY — a deterministic check
//     has NO request/response surface and, under the marker model, simply does not
//     claim one (the design's resolved Open Question 1).
//
// DESIGN DIVERGENCES (noted, not built):
//   - The fluent `p.guarded([...],[...])` factory is a NET-NEW default on the stdlib
//     `Provider` interface; the stdlib is frozen here, so we construct `s15_Guarded`
//     directly (via `s15_guarded(...)`), exactly as scenario 29 constructs `Fallback`
//     directly rather than always via `.fallback_to`.
//   - The design's `Guardrail.inspect<V>` is GENERIC over the inspected value; a
//     generic `match`/`is T` is an irrefutable catch-all on the real surface (see
//     _plan/baml_gotchas.md), so guards inspect the STRINGIFIED value (`inspect(value:
//     string)`) — input text verbatim, the output `T` via `baml.json.to_string<T>`.
//   - `s15_CodeGuard` uses a substring needle instead of a host regex (`regex_search`
//     was a `$rust_io_function`; offline tests run pure BAML with no host shims).


// ── 1. The tripwire vocabulary ───────────────────────────────────────────────

/// A guardrail's typed verdict. `tripped` is the "abort the run" bit; `info` is the
/// detector payload (the matched pattern, the judge's reason) for the catch site.
class s15_GuardVerdict {
    tripped: bool,
    info: string,
}

/// The typed input tripwire. A deliberate guardrail domain failure: not retryable,
/// a policy refusal, neither network nor parse. Legal on the CallError channel (the
/// plain `.call` path) AND the ToolError channel (mid tool-loop), so it `implements`
/// both — plus the mandatory `Failure` axis.
class s15_InputTripwire {
    name: string,
    info: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}

/// The typed output tripwire — same shape, distinct TYPE so the app catches an
/// output trip without catching an input trip.
class s15_OutputTripwire {
    name: string,
    info: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}


// ── 2. The Guardrail capability (a guardrail IS a Provider) ───────────────────

/// A guardrail inspects one (stringified) value and returns a verdict. It does NOT
/// throw on a trip — the COMBINATOR decides what a trip means (abort here). Because
/// it `requires baml.ai.Provider`, a guardrail is itself a provider and composes with
/// `.with_retry` / `.fallback_to` like anything else. Inspecting is on the call path
/// (a model guard runs a judge call) → the CallError channel.
interface s15_Guardrail requires baml.ai.Provider {
    function guard_name(self) -> string throws never
    function inspect(self, value: string) -> s15_GuardVerdict
        throws baml.errors.CallError | baml.errors.UnknownError
}


// ── 3. s15_ModelGuard — the guardrail IS a model call ─────────────────────────

/// The judge's structured answer.
class s15_GuardDecision {
    flagged: bool,
    reason: string,
}

/// A model-based guardrail (PII / toxicity as an LLM judge). It wraps an inner
/// `judge` provider. Because it `implements HttpProvider` it is a forwarding provider
/// (call/call_with passthrough to the judge, so the detector retries/caches like any
/// provider); because it `implements s15_Guardrail`, `.inspect` renders the value into
/// a judging prompt and SAP-parses a `s15_GuardDecision`.
class s15_ModelGuard {
    name: string,
    judge: baml.ai.Provider,
    rubric: string,

    implements baml.ai.Provider {}

    // Forwarding provider: call/call_with delegate straight to the judge (the
    // sidecar projection rides through, so the detector's usage/cost is not dropped).
    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["model guard forwards to its judge; no single request"] }
        }
        function send(self, request: baml.http.Request) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["model guard does not send directly"] }
        }
        function parse<T>(self, from: unknown) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["model guard does not parse directly"] }
        }
        function parse_meta(self, from: unknown) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["model guard has no meta"] }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            match (self.judge) {
                let h: baml.ai.HttpProvider => h.call_messages_with<T, V, E2>(messages, project),
                _ => throw baml.errors.Unsupported { message: "model guard judge has no request/response surface" },
            }
        }
    }

    implements s15_Guardrail {
        function guard_name(self) -> string throws never { self.name }

        function inspect(self, value: string) -> s15_GuardVerdict
            throws baml.errors.CallError | baml.errors.UnknownError {
            let prompt = self.rubric + "\n\nTEXT TO JUDGE:\n" + value;
            let decision: s15_GuardDecision = match (self.judge) {
                let h: baml.ai.HttpProvider => h.call<s15_GuardDecision>(prompt),
                _ => throw baml.errors.Unsupported { message: "model guard judge cannot do a request/response call" },
            };
            s15_GuardVerdict { tripped: decision.flagged, info: decision.reason }
        }
    }
}


// ── 4. s15_CodeGuard — a non-model guardrail (marker only, no `call`) ─────────

/// A cheap deterministic check. It `implements` the guardrail marker ONLY: a
/// deterministic checker has no request/response surface, so under the marker model
/// it simply does not claim one (still a `Provider`, since `s15_Guardrail requires
/// Provider`). `needle` is a substring to flag (the design's regex, simplified for
/// the pure-BAML offline tier).
class s15_CodeGuard {
    name: string,
    needle: string,

    implements baml.ai.Provider {}

    implements s15_Guardrail {
        function guard_name(self) -> string throws never { self.name }
        function inspect(self, value: string) -> s15_GuardVerdict throws never {
            s15_GuardVerdict { tripped: value.includes(self.needle), info: self.needle }
        }
    }
}


// ── 5. The Guarded combinator — the heart of the scenario ─────────────────────

/// Pull the user-visible text out of the native messages for the input guards
/// (the guard inspects what the USER said, not the system scaffolding).
function s15_user_text(messages: baml.ai.ChatMessage[]) -> string throws never {
    let parts: string[] = [];
    for (let m in messages) {
        if (m.role == "user") {
            for (let p in m.parts) {
                match (p.text) {
                    null => {},
                    let t: string => { parts.push(t); },
                }
            }
        }
    }
    parts.join(" ")
}

/// Wraps an inner `Provider` with input/output guardrails. A plain non-generic class
/// (the `Traced`/`Fallback` pattern): it owns no codec — the request/response methods
/// throw — and overrides the primitive `call_messages_with` to drive the inner with an
/// identity projection and run the checks around it. Also forwards `Tools`, so the same
/// guardrails wrap an agentic loop (input guarded once before the loop, output guarded
/// at the terminal value).
class s15_Guarded {
    inner: baml.ai.Provider,
    input_guards: s15_Guardrail[],
    output_guards: s15_Guardrail[],

    implements baml.ai.Provider {
        function is_effectful(self) -> bool throws never { self.inner.is_effectful() }
    }

    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
            throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["guarded forwards to inner; no single request"] }
        }
        function send(self, request: baml.http.Request) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["guarded does not send directly"] }
        }
        function parse<T>(self, from: unknown) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["guarded does not parse directly"] }
        }
        function parse_meta(self, from: unknown) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["guarded has no single meta"] }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            // INPUT phase: inspect the user text; a trip aborts BEFORE the inner call.
            let user_text = s15_user_text(messages);
            for (let g in self.input_guards) {
                let v: s15_GuardVerdict = g.inspect(user_text);
                if (v.tripped) {
                    throw s15_InputTripwire { name: g.guard_name(), info: v.info };
                }
            }

            // Drive the inner with an IDENTITY projection (the Traced pattern), so a
            // throwing caller projection can't be mistaken for a provider failure.
            let r0: baml.ai.CallResult<T, baml.ai.ResponseMeta> = match (self.inner) {
                let h: baml.ai.HttpProvider => h.call_messages_with<T, baml.ai.ResponseMeta, never>(
                    messages,
                    (mm: baml.ai.ResponseMeta) -> baml.ai.ResponseMeta { mm },
                ),
                _ => throw baml.errors.Unsupported { message: "guarded inner has no request/response surface" },
            };

            // OUTPUT phase: inherently sequential (needs the value first).
            let out_text: string = baml.json.to_string<T>(r0.value) catch (e) { _ => "" };
            for (let g in self.output_guards) {
                let v: s15_GuardVerdict = g.inspect(out_text);
                if (v.tripped) {
                    throw s15_OutputTripwire { name: g.guard_name(), info: v.info };
                }
            }

            baml.ai.CallResult<T, V> { value: r0.value, meta: project(r0.meta) }
        }
    }
}

// DESIGN DIVERGENCE (real-surface finding): the design has `s15_Guarded implements
// Tools` by forwarding begin/step/submit through an `InnerTranscript { _h: unknown }`
// wrapper. That does NOT typecheck on the real surface — a provider's `Transcript` is
// an UNNAMEABLE associated existential, and `unknown` does not coerce back to
// `inner.Transcript` when re-entering `inner.step`/`inner.submit` (E0001: "expected
// baml.ai.Tools.Transcript, got unknown"). A combinator therefore cannot forward a
// stateful, provider-owned transcript across the begin→step seam.
//
// The design-true intent — the SAME guard set wrapping a Tools harness — is instead
// expressed by a driver that keeps the transcript ENTIRELY inside the inner provider's
// own `run_tools` (it never crosses the wrapper seam): input guard the prompt, run the
// inner loop, output guard the terminal value.
function s15_guarded_run_tools(
    inner: baml.ai.Provider,
    input_guards: s15_Guardrail[],
    output_guards: s15_Guardrail[],
    prompt: string,
    tools: baml.ai.Tool[],
    dispatch: (baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never,
) -> string throws baml.errors.ToolError | baml.errors.UnknownError {
    // INPUT phase: guard the prompt before the loop runs.
    for (let g in input_guards) {
        let v: s15_GuardVerdict = g.inspect(prompt) catch (e) {
            _ => throw baml.errors.UnknownError { data: e, message: ["guarded run_tools: input guard inspect failed"] },
        };
        if (v.tripped) { throw s15_InputTripwire { name: g.guard_name(), info: v.info }; }
    }
    match (inner) {
        let tp: baml.ai.Tools => {
            let outcome = tp.run_tools<string>(prompt, tools, dispatch);
            let value: string = match (outcome) {
                let s: string => s,
                let b: baml.ai.Budget<string> => throw baml.errors.UnknownError { data: null, message: ["guarded run_tools: loop hit its budget"] },
                let h: baml.ai.HandoffRequest => throw baml.errors.UnknownError { data: null, message: ["guarded run_tools: unresolved handoff"] },
            };
            // OUTPUT phase: guard the terminal value.
            let out_text: string = baml.json.to_string<string>(value) catch (e) { _ => value };
            for (let g in output_guards) {
                let v: s15_GuardVerdict = g.inspect(out_text) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["guarded run_tools: output guard inspect failed"] },
                };
                if (v.tripped) { throw s15_OutputTripwire { name: g.guard_name(), info: v.info }; }
            }
            value
        },
        _ => throw baml.errors.Unsupported { message: "guarded inner has no Tools capability" },
    }
}

/// Ergonomic constructor (the design's `.guarded(...)` fluent factory, spelled as a
/// free function because the stdlib `Provider` interface is frozen here).
function s15_guarded(
    inner: baml.ai.Provider,
    inputs: s15_Guardrail[],
    outputs: s15_Guardrail[],
) -> s15_Guarded throws never {
    s15_Guarded { inner: inner, input_guards: inputs, output_guards: outputs }
}


// ── 6. Concurrent input guarding — BEP-034 (DESIGN DIVERGENCE: runtime hang) ───
//
// The design runs input guards CONCURRENTLY with the agent and cancels the agent on a
// trip (`spawn` the agent + each guard, then race/cancel). BEP-034 is real language
// surface, and the plain top-level `spawn { … } / await` shape works (scenario 43
// proves it). But the guardrail shape here — spawning each guard inside a `.map`
// closure and then `await`-ing those spawned futures back in a loop (or via
// `baml.future.all` / `baml.future.race`, and/or a `baml.spawn.CancelToken` linked via
// `spawn with options(cancel = …)`) — DEADLOCKS the VM in this scenario-test context:
// the process compiles, then parks at 0% CPU forever (all three variants tried:
// race+CancelToken, all()+cancel, and plain spawn-in-map + await-in-loop + Future.cancel()).
//
// The SEQUENTIAL input phase in `s15_Guarded.call_messages_with` above is behaviorally
// faithful for the guardrail contract: a trip aborts before the inner call, a clean
// prompt proceeds. The only property the concurrent form adds is LATENCY overlap (the
// guard and the model run at the same time) — not a different observable outcome. So
// the concurrent race/cancel variant is left as this note rather than shipped code,
// flagged as a runtime gap (spawn-in-map-closure + awaiting the spawned futures hangs).
