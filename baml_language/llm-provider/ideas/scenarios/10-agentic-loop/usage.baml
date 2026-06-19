// =============================================================================
// usage.baml — what an APPLICATION author writes. End-user code.
//
// Shows the full ergonomic spread:
//   * the trivial case (inherit the default run_tools — zero policy code);
//   * a step cap          (Bounded + step_count_is);
//   * a stopWhen predicate (Bounded + has_tool_call / any_of);
//   * partial-result-at-budget (Budget<T> at the call site);
//   * per-turn tool filtering (a closure handed to Bounded);
//   * portability: OpenAI AND Gemini through the SAME function bodies.
// =============================================================================

// ---- tool registry (app-defined). Tool.parameters uses the first-class `type`.
class WebSearchArgs  { query: string }
class FinalAnswerArgs { answer: string }

function tools_for_research() -> Tool[] {
  [
    Tool { name: "web_search",  description: "Search the web.",        parameters: WebSearchArgs },
    Tool { name: "final_answer", description: "Emit the final answer.", parameters: FinalAnswerArgs },
  ]
}

// the app's dispatch logic lives in ctx.dispatch; here is a registry it routes to.
function dispatch_research(calls: ToolCall[]) -> ToolResult[] {
  let out: ToolResult[] = [];
  for (let c in calls) {
    if (c.name == "web_search") {
      out = baml.list.push(out, ToolResult { id: c.id, output: do_web_search(c.args) });
    } else {
      out = baml.list.push(out, ToolResult { id: c.id, output: c.args });   // final_answer echoes
    }
  }
  out
}
function do_web_search(args: map<string, unknown>) -> unknown { $rust_io_function }


// -----------------------------------------------------------------------------
// (1) Trivial case — inherit the default loop. The app writes NOTHING about the
// loop; `.run_tools` is the companion, and the default run_tools drives it.
// Portability for free: swap the client body, the function is unchanged.
// -----------------------------------------------------------------------------

client Researcher() {
  OpenAITools { model: "gpt-5.3-codex", api_key: env.OPENAI_API_KEY }
}

client ResearcherGemini() {
  GeminiTools { model: "gemini-2.5-pro", api_key: env.GEMINI_API_KEY }
}

function Research(question: string) -> string {
  client: Researcher()
  prompt #"Answer using tools when needed: {{ question }}"#
}
// call site:
//   Research.run_tools("When did AI SDK v5 ship?", tools_for_research(), ctx)
//   -> drives the default loop until the model stops requesting tools.
// Gemini version is byte-identical except for the client:
//   function ResearchG(...) { client: ResearcherGemini() ... }


// -----------------------------------------------------------------------------
// (2) Step cap. The app wraps the provider in Bounded with step_count_is. Note:
// `Bounded` is a Provider+Tools combinator, so it slots into `client:` like any
// other. The wire code (begin/step/submit) is untouched.
// -----------------------------------------------------------------------------

client CappedResearcher() {
  Bounded {
    inner: OpenAITools { model: "gpt-5.3-codex", api_key: env.OPENAI_API_KEY },
    stop_when: step_count_is(8),                 // hard cap on round-trips
    per_turn_tools: null,
  }
}

function ResearchCapped(question: string) -> string {
  client: CappedResearcher()
  prompt #"{{ question }}"#
}
// At 8 round-trips Bounded.run_tools throws BudgetHit. If the app calls the bare
// companion it surfaces as an error; to GET the partial, see (4).


// -----------------------------------------------------------------------------
// (3) stopWhen predicate, ORed. Identical to Vercel's
//     stopWhen: [ stepCountIs(8), hasToolCall("final_answer") ].
// -----------------------------------------------------------------------------

client SmartResearcher() {
  Bounded {
    inner: OpenAITools { model: "gpt-5.3-codex", api_key: env.OPENAI_API_KEY },
    stop_when: any_of([
      step_count_is(8),
      has_tool_call("final_answer"),             // model signalled it is done
    ]),
    per_turn_tools: null,
  }
}

// Portability check: the SAME stop policy over Gemini. Only `inner` changes; the
// predicate, the loop, and this function body are unchanged.
client SmartResearcherGemini() {
  Bounded {
    inner: GeminiTools { model: "gemini-2.5-pro", api_key: env.GEMINI_API_KEY },
    stop_when: any_of([ step_count_is(8), has_tool_call("final_answer") ]),
    per_turn_tools: null,
  }
}


// -----------------------------------------------------------------------------
// (4) Partial result at the budget. Instead of the bare companion, the app calls
// the explicit `run_to_budget`, getting `T | Budget<T>`. This is the background
// "the limit signal must carry the last partial result" requirement, surfaced.
// -----------------------------------------------------------------------------

function ResearchWithPartial(question: string) -> string {
  client: SmartResearcher()
  prompt #"{{ question }}"#
}

// app-level driver: reach the combinator and ask for the budget-aware result.
function drive_research(question: string, ctx: ExecutionContext) -> string {
  let p = SmartResearcher();                      // p : Provider (existential)
  match (p) {
    let b: Bounded => {
      match (b.run_to_budget<string>(render_prompt_for(question), tools_for_research(), ctx)) {
        let done: string  => done,                            // model finished cleanly
        let cap: Budget<string> => {
          // best-effort partial; fall back to a placeholder if nothing landed yet.
          match (cap.partial) {
            let s: string => baml.string.concat("[partial @ budget] ", s),
            _             => "[no answer before budget]",
          }
        },
      }
    },
    _ => throw baml.errors.Unsupported { message: "client is not a Bounded loop" },
  }
}
function render_prompt_for(q: string) -> baml.llm.PromptAst { $rust_io_function }


// -----------------------------------------------------------------------------
// (5) Per-turn tool filtering. Drop `web_search` after it has been used once, so
// the model is nudged to commit to `final_answer` on later turns. The filter is
// a plain closure handed to Bounded; it reads the opaque History via the same
// narrow helpers the predicates use. No wire code is touched.
// -----------------------------------------------------------------------------

client FocusedResearcher() {
  Bounded {
    inner: OpenAITools { model: "gpt-5.3-codex", api_key: env.OPENAI_API_KEY },
    stop_when: step_count_is(6),
    per_turn_tools: (turn, hist) => {
      if (history_contains_tool_call(hist, "web_search")) {
        // web_search already used -> only expose final_answer from now on.
        [ Tool { name: "final_answer", description: "Emit the final answer.", parameters: FinalAnswerArgs } ]
      } else {
        tools_for_research()
      }
    },
  }
}

function ResearchFocused(question: string) -> string {
  client: FocusedResearcher()
  prompt #"{{ question }}"#
}


// -----------------------------------------------------------------------------
// (6) Combinator chain reads naturally: bound the loop, then make the underlying
// model resilient. Bounded wraps Tools; with_retry/fallback_to wrap Provider.
// Because Bounded is itself a Provider, the orchestration combinators compose
// around it. (NOTE: see evaluation.md (c) — retry around a Tools loop is subtle.)
// -----------------------------------------------------------------------------

client ResilientBounded() {
  Bounded {
    inner: OpenAITools { model: "gpt-5.3-codex", api_key: env.OPENAI_API_KEY },
    stop_when: step_count_is(10),
    per_turn_tools: null,
  }.with_retry(2)
}
