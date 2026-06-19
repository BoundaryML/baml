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
class InputTripwire  { name: string, info: unknown }
class OutputTripwire { name: string, info: unknown }


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
  function inspect<V>(self, value: V) -> GuardVerdict throws Error
}


// -----------------------------------------------------------------------------
// 3. ModelGuard — the guardrail IS a model call
// -----------------------------------------------------------------------------
//
// The common case (background §6 "The guardrail is another model"): PII /
// toxicity detection is an LLM call. ModelGuard wraps an inner Provider (the
// detector model) and turns its structured answer into a verdict. Because it
// `implements Provider`, ModelGuard.call just delegates to the inner detector;
// because it `implements Guardrail`, .inspect renders the value into a judging
// prompt and SAP-parses a Decision.

class GuardDecision {
  flagged: bool
  reason: string
}

class ModelGuard {
  guard_name: string
  judge: Provider          // the detector model — itself a Provider
  rubric: string           // "Return flagged=true if the text contains PII", etc.

  // Provider.call is a passthrough to the judge. This is what makes ModelGuard
  // reusable as an ordinary provider (you can retry/cache the detector).
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.judge.call<T>(prompt)
    }
  }

  implements Guardrail {
    function name(self) -> string { self.guard_name }

    function inspect<V>(self, value: V) -> GuardVerdict throws baml.errors.LlmClient {
      // Render the value + rubric into a judging prompt. render_guard_prompt is a
      // host helper that stringifies V and builds a PromptAst asking for a
      // GuardDecision — the judge never sees the actor's tools, only the text.
      let p: baml.llm.PromptAst = render_guard_prompt(self.rubric, value);
      let decision: GuardDecision = self.judge.call<GuardDecision>(p);
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
// is ALSO a Guardrail, so the combinator below is agnostic. Its .call has no
// meaningful "produce an answer from a prompt" — a deterministic checker is a
// degenerate provider — so it errors. (Open Question 1: does every capability
// really `requires Provider`? Here the answer is awkward; see evaluation.md.)

class CodeGuard {
  guard_name: string
  pattern: string          // e.g. an SSN / credit-card regex

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      throw baml.errors.LlmClient { message: "CodeGuard is not a generative provider" }
    }
  }

  implements Guardrail {
    function name(self) -> string { self.guard_name }
    function inspect<V>(self, value: V) -> GuardVerdict throws baml.errors.LlmClient {
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
// plain non-generic class (the Fallback/Retry pattern). It forwards Provider.call
// — adding the input/output checks around the inner .call — and forwards Tools so
// guardrails apply to an agentic loop too.
//
// Concurrency: input guardrails run CONCURRENTLY with the inner call and cancel
// it on a trip (background §6 "run the input guardrail concurrently ... cancel").
// We express this with a host race primitive `baml.sys.race_cancel`, since BAML
// has no native task/cancel surface. This is NET-NEW (see evaluation.md).

class Guarded {
  inner: Provider
  input_guards: Guardrail[]
  output_guards: Guardrail[]

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      // --- INPUT phase, run CONCURRENTLY with the agent ---------------------
      // We extract the user-visible text from the prompt to feed the input guards.
      let user_text: string = prompt_user_text(prompt);

      // race_cancel(agent_task, guard_tasks):
      //   - starts the agent task and every guard task on the host scheduler;
      //   - if any guard task resolves with tripped=true, it CANCELS the agent
      //     task and the race resolves to an InputTrip sentinel;
      //   - if the agent finishes first, all guard tasks are cancelled and the
      //     race resolves to the agent's value.
      // We model the two outcomes as a tagged union the host returns.
      let outcome: AgentValue<T> | InputTrip = baml.sys.race_cancel<T>(
        // the agent task: just the inner call
        () => { self.inner.call<T>(prompt) },
        // the guard tasks: each inspects the user text and trips on tripped
        self.input_guards,
        user_text,
      );

      let value: T = match (outcome) {
        let trip: InputTrip => {
          throw InputTripwire { name: trip.name, info: trip.info };
        },
        let ok: AgentValue<T> => { ok.value },
      };

      // --- OUTPUT phase, inherently SEQUENTIAL ------------------------------
      // Output guards need the value first, so no concurrency is possible.
      for (let g in self.output_guards) {
        let v: GuardVerdict = g.inspect<T>(value);
        if (v.tripped) {
          throw OutputTripwire { name: g.name(), info: v.info };
        }
      }
      value
    }
  }

  // Forward the Tools capability so guardrails wrap an agentic loop. We guard the
  // INPUT once (before begin) and the OUTPUT once (on the terminal value). We do
  // NOT guard every intermediate tool result — that is a different mechanism
  // (per-tool approval / canUseTool, background §6 "Where the guardrail runs").
  implements Tools {
    // The transcript is just the inner provider's transcript — we don't own one.
    type Transcript = InnerTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> InnerTranscript throws baml.errors.LlmClient {
      // Input guard BEFORE the loop starts. Sequential here (no agent running yet
      // to race against — begin() hasn't produced a value).
      let user_text: string = prompt_user_text(prompt);
      for (let g in self.input_guards) {
        let v: GuardVerdict = g.inspect<string>(user_text);
        if (v.tripped) { throw InputTripwire { name: g.name(), info: v.info }; }
      }
      match (self.inner) {
        let t: Tools => { InnerTranscript { _h: t.begin<T>(prompt, tools) } },
        _            => { throw baml.errors.LlmClient { message: "inner provider has no Tools capability" }; },
      }
    }

    function step<T>(self, t: InnerTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
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
        _ => { throw baml.errors.LlmClient { message: "inner provider has no Tools capability" }; },
      }
    }

    function submit(self, t: InnerTranscript, results: ToolResult[]) -> InnerTranscript throws baml.errors.LlmClient {
      match (self.inner) {
        let inner: Tools => { InnerTranscript { _h: inner.submit(t._h, results) } },
        _ => { throw baml.errors.LlmClient { message: "inner provider has no Tools capability" }; },
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
// 6. The race/cancel host primitive  (NET-NEW)
// -----------------------------------------------------------------------------
//
// BAML today has no task-spawn / cooperative-cancel surface. The concurrent input
// guardrail NEEDS one: start the agent and the guards together, cancel the loser.
// We declare the minimal shape and implement it at the host boundary.
//
// race_cancel<T>(agent, guards, value):
//   - `agent` is a thunk returning T (the inner call);
//   - `guards` each .inspect the `value` (the user text);
//   - returns AgentValue<T> if the agent wins, or InputTrip if a guard trips first;
//   - in BOTH cases the loser tasks are cancelled by the host.

class AgentValue<T> { value: T }
class InputTrip     { name: string, info: unknown }

// The host body: spawns the agent thunk and one task per guard on the runtime's
// scheduler, awaits the first to resolve to a terminal condition (agent done, or
// a guard with tripped=true), cancels the rest, and maps a tripped guard to
// InputTrip{ name, info }. Guards that resolve tripped=false are ignored.
function race_cancel<T>(
  agent: () -> T,
  guards: Guardrail[],
  value: string,
) -> AgentValue<T> | InputTrip throws baml.errors.LlmClient { $rust_io_function }


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
