// =============================================================================
// 15-guardrails / implement.baml  —  LIBRARY / STD-LIB AUTHOR CODE
//
// Input/output guardrails as combinators that WRAP a Provider. A tripwire is a
// typed throw. The guardrail is itself a Provider (a model call), so it composes
// with retry/fallback exactly like any other provider. We show:
//
//   - Guard:      a typed verdict + the typed tripwire exception.
//   - Guardrail:  an interface over "inspect a value, maybe trip". Its canonical
//                 impl, ModelGuard, IS a Provider — the check is an LLM call.
//   - Guarded:    the combinator. It wraps an inner Provider, runs OUTPUT guards
//                 sequentially after .call, and runs INPUT guards CONCURRENTLY
//                 with the inner call, cancelling the inner on a trip.
//
// Build on the canonical primitives in _conventions.md (Provider, HttpProvider,
// Tools, the combinator pattern). New surface introduced here is flagged in
// evaluation.md as net-new.
// =============================================================================


// -----------------------------------------------------------------------------
// 1. The tripwire vocabulary
// -----------------------------------------------------------------------------
//
// A guardrail returns a typed verdict. `tripped` is the "abort the run" bit;
// `info` is whatever the detector produced (the PII spans, the toxicity score)
// so the catch site can log/explain it. This mirrors the SDK's
// GuardrailFunctionOutput { output_info, tripwire_triggered }.

class GuardVerdict {
  tripped: bool
  info: unknown          // detector payload: PII spans, toxicity score, etc.
}

// The typed throw. We keep the input/output distinction in the TYPE (not just a
// field) so the application can `catch` one without the other — matching the
// SDK's two distinct exception classes InputGuardrailTripwireTriggered /
// OutputGuardrailTripwireTriggered.
//
// A tripwire is a deliberate guardrail DOMAIN failure (like a Refused), raised
// both on the plain-call path (Guarded.call -> CallError) AND mid tool-loop
// (Guarded.step -> ToolError), so each implements BOTH capability interfaces. It
// is neither a transport nor a parse fault, so every classifier is false.
class InputTripwire {
  name: string
  info: unknown
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.ToolError {}
}
class OutputTripwire {
  name: string
  info: unknown
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.ToolError {}
}


// -----------------------------------------------------------------------------
// 2. The Guardrail capability
// -----------------------------------------------------------------------------
//
// A guardrail inspects ONE value and returns a verdict. It is generic over the
// value it inspects: an input guardrail inspects the rendered prompt's user
// text (string); an output guardrail inspects the agent's typed result T.
//
// Crucially `Guardrail requires Provider`: a guardrail is itself a provider, so
// the SAME thing can be .with_retry'd, .cached'd, or fed into a fallback chain.
// The interface adds ONE method on top of Provider.

interface Guardrail requires Provider {
  // The guardrail's own name, for the tripwire payload.
  function name(self) -> string

  // Inspect a value. Returns a verdict; does NOT throw on a trip — the COMBINATOR
  // decides what a trip means (abort vs rewrite vs log). This keeps the
  // "what a trip does" policy (background §6 "What a trip does") at the wrap site.
  // Inspecting is on the call path (a model guard does an LLM judge call) -> CallError.
  function inspect<V>(self, value: V) -> GuardVerdict
      throws baml.ExtendUnknownError<baml.errors.CallError>
}


// -----------------------------------------------------------------------------
// 3. ModelGuard — the guardrail IS a model call
// -----------------------------------------------------------------------------
//
// The common case (background §6 "The guardrail is another model"): PII /
// toxicity detection is an LLM call. ModelGuard wraps an inner Provider (the
// detector model) and turns its structured answer into a verdict. Because it
// `implements HttpProvider`, ModelGuard.call/.call_with just delegate to the
// inner detector (forwarding the request/response surface — ModelGuard is a
// forwarding provider); because it `implements Guardrail`, .inspect renders the
// value into a judging prompt and SAP-parses a Decision.

class GuardDecision {
  flagged: bool
  reason: string
}

class ModelGuard {
  guard_name: string
  judge: Provider          // the detector model — itself a Provider
  rubric: string           // "Return flagged=true if the text contains PII", etc.

  // HttpProvider.call/.call_with are a passthrough to the judge. This is what
  // makes ModelGuard reusable as an ordinary request/response provider (you can
  // retry/cache the detector). call_with FORWARDS the sidecar projection to the
  // judge so usage/cost of the detector model is not dropped.
  implements HttpProvider {
    type Body = unknown

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      match (self.judge) {
        let h: HttpProvider => { return h.call_with<T, U>(prompt, project); },
        _                   => { throw baml.errors.Unsupported { message: "model guard judge has no request/response surface" }; },
      }
    }

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  implements Guardrail {
    function name(self) -> string { self.guard_name }

    function inspect<V>(self, value: V) -> GuardVerdict
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // Render the value + rubric into a judging prompt. render_guard_prompt is a
      // host helper that stringifies V and builds a PromptAst asking for a
      // GuardDecision — the judge never sees the actor's tools, only the text.
      let p: baml.llm.PromptAst = render_guard_prompt(self.rubric, value);
      // The judge is a bare Provider (marker); `call` lives on HttpProvider, so
      // we match for it. A judge that cannot do request/response is unusable here.
      let decision: GuardDecision = match (self.judge) {
        let h: HttpProvider => h.call<GuardDecision>(p),
        _                   => throw baml.errors.Unsupported { message: "model guard judge cannot do a request/response call" },
      };
      GuardVerdict { tripped: decision.flagged, info: decision }
    }
  }
}

// Host boundary: turn (rubric, value) into a judging PromptAst. Pure string work,
// but it stringifies an arbitrary V, so it lives at the host edge like render_prompt.
function render_guard_prompt<V>(rubric: string, value: V) -> baml.llm.PromptAst { $rust_io_function }


// -----------------------------------------------------------------------------
// 4. CodeGuard — a non-model guardrail (regex/deterministic)
// -----------------------------------------------------------------------------
//
// Not every guardrail is a model. A cheap deterministic check (a regex for SSNs)
// is ALSO a Guardrail, so the combinator below is agnostic. A deterministic
// checker has NO request/response surface — and under the marker model it simply
// does not claim one: `CodeGuard` implements `Guardrail` only. It is still a
// `Provider` (the marker) because `Guardrail requires Provider`, so it composes
// with the other guards in a list — but it carries no fake/degenerate `call`.
// (This is the resolution of the old Open Question 1: a capability `requires
// Provider` the MARKER, not a `call`; see evaluation.md.)

class CodeGuard {
  guard_name: string
  pattern: string          // e.g. an SSN / credit-card regex

  implements Guardrail {
    function name(self) -> string { self.guard_name }
    function inspect<V>(self, value: V) -> GuardVerdict
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let hit: bool = regex_search(self.pattern, value);   // host: stringify V, run regex
      GuardVerdict { tripped: hit, info: self.pattern }
    }
  }
}

function regex_search<V>(pattern: string, value: V) -> bool { $rust_io_function }


// -----------------------------------------------------------------------------
// 5. The Guarded combinator — the heart of the scenario
// -----------------------------------------------------------------------------
//
// Guarded wraps an inner Provider with lists of input/output guardrails. It is a
// plain non-generic class (the Fallback/Retry pattern). It forwards the
// HttpProvider request/response surface — adding the input/output checks around
// the inner .call_with — and forwards Tools so guardrails apply to an agentic
// loop too.
//
// Concurrency: input guardrails run CONCURRENTLY with the inner call and cancel
// it on a trip (background §6 "run the input guardrail concurrently ... cancel").
// This is plain structured concurrency (BEP-034): `spawn` the inner call and each
// guard, `baml.future.race` them, and use a `baml.spawn.CancelToken` so a tripping
// guard cancels the inner call. No host primitive — it's a shipped language feature.

class Guarded {
  inner: Provider
  input_guards: Guardrail[]
  output_guards: Guardrail[]

  implements HttpProvider {
    // Guarded only forwards the inner provider's request/response surface; it has
    // no codec of its own. call_with is the primitive and threads `project`
    // straight through to the inner's call_with, so the inner detector/agent's
    // usage/cost sidecar is NOT dropped by the guard wrapper (the P1 fix).
    type Body = unknown

    function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      // --- INPUT phase, run CONCURRENTLY with the agent (BEP-034) -----------
      // We extract the user-visible text from the prompt to feed the input guards.
      let user_text: string = prompt_user_text(prompt);

      // A cooperative cancel token wired into the agent's spawn. When a guard
      // trips it fires; the agent's next `await` then throws baml.panics.Cancelled
      // (cancels the in-flight HTTP request / subprocess turn cooperatively).
      let cancel = baml.spawn.CancelToken.new();

      // The agent task: forward call_with to the inner request/response provider.
      // `cancel = cancel` links the token so a tripping guard can stop it. Its
      // settled value is the (T, U) answer; its error rides the CallError channel.
      let agent: baml.future.Future<(T, U), baml.errors.CallError> =
        spawn with baml.spawn.options(cancel = cancel) {
          match (self.inner) {
            let h: HttpProvider => h.call_with<T, U>(prompt, project),
            _                   => throw baml.errors.Unsupported { message: "inner provider has no request/response surface" },
          }
        };

      // Each guard task: inspect the user text and, on a trip, fire the agent's
      // cancel token and throw the typed InputTripwire (the throw IS the trip
      // signal — InputTripwire implements CallError, so it shares the agent's
      // error channel). A NON-tripping guard has nothing to contribute, so it
      // mirrors the agent (`await agent`): it can only settle with the agent's own
      // (T, U), so it never beats the agent in the race. Thus only the agent or a
      // TRIPPING guard ever wins.
      let guards: baml.future.Future<(T, U), baml.errors.CallError>[] =
        self.input_guards.map((g) -> {
          spawn {
            let v: GuardVerdict = g.inspect<string>(user_text);
            if (v.tripped) {
              cancel.cancel();                                   // stop the agent
              throw InputTripwire { name: g.name(), info: v.info };
            }
            await agent                                          // mirror the agent
          }
        });

      // Race the agent against the guards (Promise.race): the FIRST to settle wins
      // and the losers are cancelled. A tripping guard settles first by throwing
      // InputTripwire (and its cancel.cancel() has already stopped the agent);
      // otherwise the agent settles first with (T, U) and race cancels the guards.
      let (value, sidecar): (T, U) = await baml.future.race(
        [agent].concat(guards),
      );

      // --- OUTPUT phase, inherently SEQUENTIAL ------------------------------
      // Output guards need the value first, so no concurrency is possible.
      for (let g in self.output_guards) {
        let v: GuardVerdict = g.inspect<T>(value);
        if (v.tripped) {
          throw OutputTripwire { name: g.name(), info: v.info };
        }
      }
      (value, sidecar)
    }

    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
      v
    }
  }

  // Forward the Tools capability so guardrails wrap an agentic loop. We guard the
  // INPUT once (before begin) and the OUTPUT once (on the terminal value). We do
  // NOT guard every intermediate tool result — that is a different mechanism
  // (per-tool approval / canUseTool, background §6 "Where the guardrail runs").
  implements Tools {
    // The transcript is just the inner provider's transcript — we don't own one.
    type Transcript = InnerTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> InnerTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      // Input guard BEFORE the loop starts. Sequential here (no agent running yet
      // to race against — begin() hasn't produced a value).
      let user_text: string = prompt_user_text(prompt);
      for (let g in self.input_guards) {
        let v: GuardVerdict = g.inspect<string>(user_text);
        if (v.tripped) { throw InputTripwire { name: g.name(), info: v.info }; }
      }
      match (self.inner) {
        let t: Tools => { InnerTranscript { _h: t.begin<T>(prompt, tools) } },
        _            => { throw baml.errors.Unsupported { message: "inner provider has no Tools capability" }; },
      }
    } catch (e) {
      // A guard's .inspect throws on the CallError channel (a judge rate-limit, an
      // Unsupported judge). That is NOT a ToolError, so we normalize it into THIS
      // method's ToolError channel. InputTripwire / Unsupported are known ToolErrors
      // and pass through with_message unchanged — only the alien CallError is wrapped.
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "guarded begin: input guard failed");
    }

    function step<T>(self, t: InnerTranscript) -> T | ToolCalls
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      match (self.inner) {
        let inner: Tools => {
          match (inner.step<T>(t._h)) {
            let calls: ToolCalls => { calls },
            let value: T => {
              // Terminal value reached -> OUTPUT guard here, at the loop exit.
              for (let g in self.output_guards) {
                let v: GuardVerdict = g.inspect<T>(value);
                if (v.tripped) { throw OutputTripwire { name: g.name(), info: v.info }; }
              }
              value
            },
          }
        },
        _ => { throw baml.errors.Unsupported { message: "inner provider has no Tools capability" }; },
      }
    } catch (e) {
      // Same normalization: the output guard's CallError-channel faults become
      // UnknownError on the ToolError channel; OutputTripwire / Unsupported pass through.
      _ => throw baml.UnknownError.with_message<baml.errors.ToolError>(e, "guarded step: output guard failed");
    }

    function submit(self, t: InnerTranscript, results: ToolResult[]) -> InnerTranscript
        throws baml.ExtendUnknownError<baml.errors.ToolError> {
      match (self.inner) {
        let inner: Tools => { InnerTranscript { _h: inner.submit(t._h, results) } },
        _ => { throw baml.errors.Unsupported { message: "inner provider has no Tools capability" }; },
      }
    }
    // run_tools is inherited from Tools' default loop and dispatches through the
    // overridden step/begin/submit above.
  }
}

// Opaque wrapper around the inner provider's transcript. We hold it as `unknown`
// because the inner Transcript type is an associated existential we can't name.
class InnerTranscript { _h: unknown }

// Host helper: pull the user-authored text out of a rendered prompt for the input
// guard. (The guard inspects what the USER said, not the system scaffolding.)
function prompt_user_text(prompt: baml.llm.PromptAst) -> string { $rust_io_function }


// -----------------------------------------------------------------------------
// 6. The concurrent input phase is plain BEP-034 structured concurrency
// -----------------------------------------------------------------------------
//
// There is NO host race primitive and NO net-new concurrency surface. The
// "run input guards concurrently with the agent, cancel the agent on a trip"
// requirement is expressed with shipped language features (see _conventions.md
// "Concurrency"):
//   - `spawn { ... }` starts the agent and each guard concurrently, returning a
//     `baml.future.Future` immediately;
//   - `baml.spawn.CancelToken` is the cooperative cancel signal — a tripping
//     guard fires it and the agent's next `await` throws `baml.panics.Cancelled`;
//   - `baml.future.race` settles on the first task to finish and cancels the rest.
// All of this lives ABOVE the `$rust_io_function` line, in BAML itself.


// -----------------------------------------------------------------------------
// 7. The .guarded() combinator method  (NET-NEW default on Provider)
// -----------------------------------------------------------------------------
//
// To get the Iterator.map ergonomics (`p.with_retry(2).guarded(...)`) we extend
// the Provider interface with one more default method. This is the SAME shape as
// with_retry/fallback_to/cached in the spine — it just constructs a Guarded.
//
// (In real code this method body would be merged into the canonical Provider
// interface; we show it standalone here to make the net-new surface explicit.)

interface GuardrailExt requires Provider {
  function guarded(self, inputs: Guardrail[], outputs: Guardrail[]) -> Guarded {
    Guarded { inner: self, input_guards: inputs, output_guards: outputs }
  }
}
