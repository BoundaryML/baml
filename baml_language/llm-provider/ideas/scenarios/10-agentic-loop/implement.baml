// =============================================================================
// implement.baml — the agentic loop + stop conditions, library/std-lib code.
//
// Builds on the canonical primitives in ../_conventions.md (Provider, Tools,
// ToolCall/ToolResult/ToolCalls, Tool, ExecutionContext). The teaching points:
//
//   * `run_tools` is the DEFAULT method on `Tools` — the loop itself.
//   * the wire threading (assistant turn + `phase` metadata, id-vs-name
//     correlation, Anthropic positional tool_result order) lives BELOW the
//     ToolCall/ToolResult seam, inside begin/step/submit.
//   * stop-policy is OVERRIDDEN by replacing `run_tools` only — never the wire.
//   * `step -> T | ToolCalls` is the exact shape of `Iterator.next -> Item | Done`.
//   * partial-result-at-budget and per-turn tool filtering are policy, so they
//     live in a `run_tools` override and never touch threading.
// =============================================================================

// -----------------------------------------------------------------------------
// NET-NEW host surface this scenario needs on top of _conventions.md.
// -----------------------------------------------------------------------------

// A stop predicate sees the accumulated, provider-OWNED history as an opaque
// snapshot, plus a step counter. It mirrors Vercel's `(steps) => boolean`, but
// the "steps" are an opaque `History` the provider hands out — the loop never
// interprets it. (net-new: §7's ExecutionContext gains nothing; this is a new
// callback type, defined here.)
class StepInfo {
  step: int                  // number of completed model round-trips so far
  history: History           // opaque, provider-owned snapshot (see below)
}

// Opaque, provider-owned history snapshot. The loop and the stop predicate can
// pass it around and ask the provider narrow questions through `Tools` helpers,
// but they cannot read the wire format. This is the same move as `Transcript`:
// the threading discipline (assistant turn, `phase`, id correlation) stays
// inside the provider. (net-new type — a read-only view companion to Transcript.)
class History { _data: $rust_type }

// The result of a bounded loop. `Done` means the model emitted a typed value;
// `Budget` means we hit the cap and are returning the best-effort partial.
// This mirrors `step`'s `T | ToolCalls`, one level up: the OUTER loop's
// next() -> Item(final) | Done(budget-hit-with-partial).
// (net-new: the "partial result at the cap" requirement from background §2.)
class Budget<T> {
  partial: T?                // best-effort answer the model had in flight, if any
  steps_taken: int
  history: History
}

// A per-turn tool filter: given the turn index and the running history, return
// the tools to expose on THIS turn. Lets a loop drop a tool after first use, or
// gate tools on later turns (background §2 "per-turn tool filtering").
// (net-new: a callback type; the base loop ignores it.)
// signature handed in as a plain BAML closure: (int, History) -> Tool[]


// -----------------------------------------------------------------------------
// The Tools capability, restated with the divergences made real.
//
// (interface itself is canonical — from _conventions.md / proposal §7. We
//  restate the default `run_tools` here as the baseline the overrides replace.)
// -----------------------------------------------------------------------------

interface Tools requires Provider {
  type Transcript

  function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript throws Error
  function step<T>(self, t: Transcript) -> T | ToolCalls throws Error   // next() -> Item | Done
  function submit(self, t: Transcript, results: ToolResult[]) -> Transcript throws Error

  // A read-only projection of the transcript so policy code (stop predicates,
  // tool filters) can observe progress without touching the wire format.
  // Default: provider supplies it; the existence is the net-new bit.
  function history(self, t: Transcript) -> History throws Error

  // ---- the loop, as a DEFAULT method. This is the baseline every provider
  // inherits; an app overrides run_tools (NOT begin/step/submit) to change
  // dispatch policy. ----
  function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T throws Error {
    let t = self.begin<T>(prompt, tools);
    while (true) {
      match (self.step<T>(t)) {
        let calls: ToolCalls => { t = self.submit(t, ctx.dispatch(calls.calls)); },
        let value: T         => { return value; },
      }
    }
    baml.sys.panic("unreachable")
  }
}


// -----------------------------------------------------------------------------
// Provider #1: OpenAI chat-completions tool loop.
//
// Divergence shown:
//   * correlation by `tool_call_id` (id-keyed).
//   * the assistant turn is preserved WITH its `phase` label and re-sent.
//   * `step` decides Item-vs-Done by "did the assistant turn carry tool_calls".
// -----------------------------------------------------------------------------

class OpenAITools {
  model: string
  api_key: string

  // The OpenAI transcript is the literal `messages` array plus the tool schemas.
  // Each assistant turn keeps its `phase` ("commentary" | "final_answer") so the
  // model can tell its own progress notes from the final result on re-send.
  implements Tools {
    type Transcript = OpenAITranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> OpenAITranscript throws baml.errors.LlmClient {
      // render the prompt into the OpenAI `messages` shape; attach the tool
      // schemas in OpenAI's `{type:"function", function:{...}}` envelope.
      let msgs = baml.llm.openai_messages_from(prompt);
      OpenAITranscript {
        model: self.model,
        api_key: self.api_key,
        messages: msgs,
        tools: tools,
      }
    }

    function step<T>(self, t: OpenAITranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      // one model round-trip. The host call returns the raw assistant turn,
      // INCLUDING tool_calls (with their ids) and the `phase` label.
      let turn: OpenAIAssistantTurn = openai_chat_once(t);
      // CRITICAL: preserve the assistant turn AND its phase before we branch,
      // so the next request re-sends commentary vs final_answer correctly.
      t.messages = baml.list.push(t.messages, openai_assistant_message(turn));
      if (baml.list.is_empty(turn.tool_calls)) {
        // no tools requested -> the model is done. Parse the final content as T.
        return baml.sap.parse<T>(turn.content);
      }
      // Done-sentinel: surface the calls (id is OPAQUE to the loop above).
      ToolCalls {
        calls: baml.list.map(turn.tool_calls, c => ToolCall {
          id: c.id,                     // OpenAI's `tool_call_id` — id-keyed correlation
          name: c.fn_name,
          args: c.arguments,
        }),
      }
    }

    function submit(self, t: OpenAITranscript, results: ToolResult[]) -> OpenAITranscript throws baml.errors.LlmClient {
      // id-keyed: each result becomes a `{role:"tool", tool_call_id: id, ...}`
      // message. Order does not matter for OpenAI because correlation is by id.
      for (let r in results) {
        t.messages = baml.list.push(t.messages, OpenAIMessage {
          role: "tool",
          tool_call_id: r.id,
          content: baml.json.stringify(r.output),
        });
      }
      t
    }

    function history(self, t: OpenAITranscript) -> History throws baml.errors.LlmClient {
      // wrap the messages array as an opaque snapshot. Policy code can ask
      // narrow questions (last_tool_called, etc.) via free helpers below.
      History { _data: openai_history_snapshot(t.messages) }
    }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
}

class OpenAITranscript {
  model: string
  api_key: string
  messages: OpenAIMessage[]
  tools: Tool[]
}
class OpenAIMessage {
  role: string
  content: string?
  tool_call_id: string?       // present on role:"tool"
  phase: string?              // present on role:"assistant": "commentary" | "final_answer"
}
class OpenAIAssistantTurn {
  content: string?
  tool_calls: OpenAIRawCall[]
  phase: string?              // the label we MUST keep and re-send
}
class OpenAIRawCall {
  id: string
  fn_name: string
  arguments: map<string, unknown>
}


// -----------------------------------------------------------------------------
// Provider #2: Gemini tool loop.
//
// Divergence shown (the reason threading can't be lifted out):
//   * Gemini has NO call ids. Results correlate by FUNCTION NAME + POSITION.
//     The seam still uses ToolCall.id, so this provider MINTS synthetic ids on
//     the way out and RESOLVES them back to (name, position) on the way in.
//   * The assistant turn is a `model` role `functionCall` part; there is no
//     `phase` label, so it preserves the parts verbatim instead.
// -----------------------------------------------------------------------------

class GeminiTools {
  model: string
  api_key: string

  implements Tools {
    type Transcript = GeminiTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> GeminiTranscript throws baml.errors.LlmClient {
      GeminiTranscript {
        model: self.model,
        api_key: self.api_key,
        contents: baml.llm.gemini_contents_from(prompt),
        tools: tools,
        pending: [],            // (synthetic_id -> name) correlation table for the in-flight turn
      }
    }

    function step<T>(self, t: GeminiTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      let turn: GeminiModelTurn = gemini_generate_once(t);
      // preserve the model turn verbatim (the functionCall parts ARE the history).
      t.contents = baml.list.push(t.contents, gemini_model_content(turn));
      if (baml.list.is_empty(turn.function_calls)) {
        return baml.sap.parse<T>(turn.text);
      }
      // Gemini gives us (name, position) but no id. MINT a synthetic id per call
      // and remember the name+position so `submit` can put results back in order.
      let pending: GeminiPending[] = [];
      let calls: ToolCall[] = [];
      for (let i in baml.range.until(baml.list.len(turn.function_calls))) {
        let fc = baml.list.at(turn.function_calls, i);
        let synthetic = baml.uuid.v4();
        pending = baml.list.push(pending, GeminiPending { id: synthetic, name: fc.name, position: i });
        calls = baml.list.push(calls, ToolCall { id: synthetic, name: fc.name, args: fc.args });
      }
      t.pending = pending;
      ToolCalls { calls: calls }
    }

    function submit(self, t: GeminiTranscript, results: ToolResult[]) -> GeminiTranscript throws baml.errors.LlmClient {
      // RESOLVE synthetic ids back to (name, position). Gemini wants one `user`
      // content with functionResponse parts, ordered by the original call order.
      let ordered: GeminiFunctionResponse[] = [];
      for (let p in t.pending) {                       // pending is already in position order
        let r = find_result_by_id(results, p.id);      // the id the loop echoed back
        ordered = baml.list.push(ordered, GeminiFunctionResponse {
          name: p.name,                                // correlation key on the wire
          response: r.output,
        });
      }
      t.contents = baml.list.push(t.contents, gemini_function_response_content(ordered));
      t.pending = [];
      t
    }

    function history(self, t: GeminiTranscript) -> History throws baml.errors.LlmClient {
      History { _data: gemini_history_snapshot(t.contents) }
    }
  }

  implements ChatModelOptions { model as model, api_key as api_key }
}

class GeminiTranscript {
  model: string
  api_key: string
  contents: GeminiContent[]
  tools: Tool[]
  pending: GeminiPending[]    // per-turn (synthetic-id -> name+position) table
}
class GeminiPending { id: string, name: string, position: int }
class GeminiContent { role: string, _parts: $rust_type }
class GeminiModelTurn { text: string?, function_calls: GeminiRawCall[] }
class GeminiRawCall { name: string, args: map<string, unknown> }
class GeminiFunctionResponse { name: string, response: unknown }


// -----------------------------------------------------------------------------
// Combinator/policy layer — the WHOLE POINT of the scenario.
//
// Each of these is a `run_tools` OVERRIDE. They change dispatch policy and never
// touch begin/step/submit — i.e. they ride above the ToolCall/ToolResult seam,
// so they work for OpenAI AND Gemini unchanged.
// -----------------------------------------------------------------------------

// A stop predicate is just a closure: (StepInfo) -> bool. We provide the two
// standard constructors plus an OR-combinator (background §2: "predicates OR
// together"). These are free functions, not provider state.
function step_count_is(n: int) -> (StepInfo) -> bool {
  info => info.step >= n
}

// hasToolCall("finalAnswer") — stop once a given tool was requested this run.
function has_tool_call(name: string) -> (StepInfo) -> bool {
  info => history_contains_tool_call(info.history, name)
}

function any_of(preds: ((StepInfo) -> bool)[]) -> (StepInfo) -> bool {
  info => {
    for (let p in preds) {
      if (p(info)) { return true; }
    }
    false
  }
}


// The bounded loop as a capability decorator. `Bounded` wraps any `Tools`
// provider and REPLACES run_tools with a stop-predicate-driven loop that returns
// `Budget<T>` instead of bare `T`, so the caller gets the partial at the cap.
//
// It is a plain class that forwards everything else to its inner provider. The
// inner threading (begin/step/submit) is reused VERBATIM — this is the proof
// that stop-policy and wire-policy are separable.
class Bounded {
  inner: Tools                          // the existential Tools provider it wraps
  stop_when: (StepInfo) -> bool         // post-step predicate
  per_turn_tools: ((int, History) -> Tool[])?   // optional per-turn tool filter

  // Bounded is itself a Provider (every combinator is) so it chains.
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      self.inner.call<T>(prompt)
    }
  }

  // Bounded forwards the Tools capability but with the OVERRIDDEN run_tools.
  // begin/step/submit/history delegate straight through — NO rewrite of wire code.
  implements Tools {
    type Transcript = BoundedTranscript

    function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> BoundedTranscript throws baml.errors.LlmClient {
      BoundedTranscript { inner: self.inner.begin<T>(prompt, tools), all_tools: tools }
    }
    function step<T>(self, t: BoundedTranscript) -> T | ToolCalls throws baml.errors.LlmClient {
      self.inner.step<T>(t.inner)
    }
    function submit(self, t: BoundedTranscript, results: ToolResult[]) -> BoundedTranscript throws baml.errors.LlmClient {
      t.inner = self.inner.submit(t.inner, results);
      t
    }
    function history(self, t: BoundedTranscript) -> History throws baml.errors.LlmClient {
      self.inner.history(t.inner)
    }

    // THE OVERRIDE. Same skeleton as the default run_tools, but:
    //   (1) it counts steps and consults stop_when AFTER each step;
    //   (2) it re-filters the tool set per turn if per_turn_tools is set;
    //   (3) at the cap it returns the best-effort partial via Budget<T>, instead
    //       of looping forever or throwing the partial away.
    // It returns `T`, so to expose the Budget it throws a typed BudgetHit that
    // carries the partial (see run_bounded below for the ergonomic surface).
    function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T throws baml.errors.LlmClient {
      let bt = self.begin<T>(prompt, tools);
      let step_no = 0;
      while (true) {
        // per-turn tool filtering: recompute the exposed tools for THIS turn.
        // (We re-begin the inner turn's tool set only if a filter is present;
        //  the threading impl reads t.tools each round, so this is a field poke.)
        match (self.per_turn_tools) {
          let f: (int, History) -> Tool[] => {
            let snap = self.inner.history(bt.inner);
            inner_set_tools(bt.inner, f(step_no, snap));
          },
          _ => {},
        }

        match (self.inner.step<T>(bt.inner)) {
          let calls: ToolCalls => {
            bt.inner = self.inner.submit(bt.inner, ctx.dispatch(calls.calls));
            step_no = step_no + 1;
            // POST-STEP predicate (Vercel semantics): runs after tools dispatched.
            let info = StepInfo { step: step_no, history: self.inner.history(bt.inner) };
            if (self.stop_when(info)) {
              // budget hit. Best-effort partial = last assistant text if any.
              throw BudgetHit {
                partial_json: history_last_assistant_text(info.history),
                steps_taken: step_no,
                history: info.history,
              };
            }
          },
          let value: T => { return value; },           // model produced the typed answer
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  // A static helper that runs the bounded loop and turns the BudgetHit throw into
  // a typed `Budget<T>` value, so the app gets `T | Budget<T>` without catch noise.
  function run_to_budget<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T | Budget<T> throws baml.errors.LlmClient {
    let v: T = self.run_tools<T>(prompt, tools, ctx) catch (e) {
      let b: BudgetHit => {
        // turn the partial JSON into a best-effort T (may be null if nothing yet).
        let partial: T? = baml.sap.parse_opt<T>(b.partial_json);
        return Budget<T> { partial: partial, steps_taken: b.steps_taken, history: b.history };
      },
    };
    v
  }
}

class BoundedTranscript {
  inner: Tools.Transcript     // the wrapped provider's opaque transcript
  all_tools: Tool[]
}

// thrown internally by the override when the budget is hit; carries the partial.
class BudgetHit {
  partial_json: string?
  steps_taken: int
  history: History
}


// -----------------------------------------------------------------------------
// Free helpers over the opaque History. These are the ONLY way policy code reads
// progress; they hide the per-provider wire format (OpenAI messages vs Gemini
// contents) behind narrow queries. Host-backed.
// -----------------------------------------------------------------------------

function history_contains_tool_call(h: History, name: string) -> bool { $rust_io_function }
function history_last_assistant_text(h: History) -> string?           { $rust_io_function }
function find_result_by_id(results: ToolResult[], id: string) -> ToolResult { $rust_io_function }
function inner_set_tools(t: Tools.Transcript, tools: Tool[]) -> null    { $rust_io_function }

// host boundaries used by the providers above
function openai_chat_once(t: OpenAITranscript) -> OpenAIAssistantTurn    { $rust_io_function }
function openai_assistant_message(turn: OpenAIAssistantTurn) -> OpenAIMessage { $rust_io_function }
function openai_history_snapshot(msgs: OpenAIMessage[]) -> $rust_type     { $rust_io_function }
function gemini_generate_once(t: GeminiTranscript) -> GeminiModelTurn     { $rust_io_function }
function gemini_model_content(turn: GeminiModelTurn) -> GeminiContent     { $rust_io_function }
function gemini_function_response_content(rs: GeminiFunctionResponse[]) -> GeminiContent { $rust_io_function }
function gemini_history_snapshot(cs: GeminiContent[]) -> $rust_type       { $rust_io_function }
