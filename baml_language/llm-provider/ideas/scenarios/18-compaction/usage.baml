// =============================================================================
// usage.baml — what an APPLICATION author writes to compact a live context.
//
// Shows the ergonomic spread:
//   * server-managed compaction — a config field, ZERO client-side code;
//   * manual compaction at a milestone — the `.compact` companion;
//   * the AutoCompact combinator carrying the window between turns;
//   * portability: OpenAI (replacement), Anthropic (compaction/editing), Gemini
//     (roll-your-own summary) through the SAME milestone-driven driver;
//   * the place portability LEAKS: Anthropic's prune-in-place edit mode has a
//     different shape and needs its own companion.
// =============================================================================

// -----------------------------------------------------------------------------
// (1) Server-managed compaction — the cheapest case. The app sets a threshold
// FIELD on the provider and otherwise does nothing: the server compacts its own
// stored chain past the bound. No `.compact` call, no window to carry. This is
// the "client keeps doing exactly what it did before" mode.
// -----------------------------------------------------------------------------

client AutoServerSide() {
  OpenAIResponses {
    model: "gpt-5",
    api_key: env.OPENAI_API_KEY,
    compact_threshold: 100000,        // server auto-compacts the stored chain past ~100k
  }
}

function Debug(step: string) -> string {
  client: AutoServerSide()
  prompt #"{{ step }}"#
}
// call site is a plain `.call` — Debug("check the refund path"). The compaction is
// invisible: it happens server-side on the stored chain. Nothing in this function
// or its companions changes.


// -----------------------------------------------------------------------------
// (2) Manual compaction at a milestone — the `.compact` companion on a client.
// The app holds the window and decides WHEN to shrink it (after a debugging phase
// finishes). This is the manual mode where the caller's judgment drives timing.
//
// Because `compact` is a capability method, the app reaches it the same way it
// reaches `.run_tools`: a companion that `match`es the provider against the
// Compaction capability.
// -----------------------------------------------------------------------------

client OpenAICompactor() {
  OpenAIResponses { model: "gpt-5", api_key: env.OPENAI_API_KEY, compact_threshold: null }
}

// the companion: shrink the held window. Falls through to an error if the client's
// provider can't compact (there is no honest `.call` fallback for replacement
// state — you cannot fabricate machine state from a single answer).
function CompactNow(window: WindowItems) -> Compacted
    throws baml.ExtendUnknownError<baml.errors.CompactionError> {
  let p = OpenAICompactor();                       // p : Provider (existential)
  match (p) {
    let c: Compaction => {
      let w = c.window_of(window);
      c.compact(w)                                 // opaque output; pass forward verbatim
    },
    // Unsupported implements every capability-error interface (incl. CompactionError),
    // so it is legal on this channel and the caller catches it by its concrete type.
    _ => throw baml.errors.Unsupported { message: "client cannot compact" },
  }
}

// app driver: a milestone just completed, so compact and adopt the result. The app
// NEVER reads compacted.output — it splices it ahead of the next user turn as-is.
function after_milestone(window: WindowItems, next_user_turn: WindowItems) -> WindowItems
    throws baml.ExtendUnknownError<baml.errors.CompactionError> {
  let result = CompactNow(window) catch (e) {
    // most-specific-first: capability-negotiation failure, then the classified
    // CompactionError interface, then the universal escape hatch.
    let u: baml.errors.Unsupported     => throw u;                       // no compaction seam at all
    let c: baml.errors.CompactionError => {
      // a transient network/rate-limit compaction failure is non-fatal: skip this
      // milestone and carry the un-compacted window forward unchanged.
      if (c.is_network_error() || c.is_rate_limit()) { return concat_items(window, next_user_turn); }
      throw c;
    }
    let e: baml.UnknownError           => { report_compaction_failure(e.message); throw e; }
  };
  if (result.busts_cache_prefix) {
    log_cache_miss(result.before_tokens, result.after_tokens);   // the turn after pays a miss
  }
  // ordering contract: compacted.output FIRST, then the new turn.
  concat_items(result.output, next_user_turn)
}


// -----------------------------------------------------------------------------
// (3) The AutoCompact combinator — carry the window between turns and compact at
// milestones, portable across providers. The combinator owns the window; the app
// just calls `checkpoint()` after a sub-task and `append_turn()` for new input.
//
// PORTABILITY PROOF: the SAME driver body works for OpenAI replacement, Anthropic
// summarize-style compaction, and Gemini roll-your-own summary — because all three
// implement `Compaction` and the driver only touches the opaque seam.
// -----------------------------------------------------------------------------

// OpenAI — provider-produced replacement state.
function make_openai_session(initial: WindowItems) -> AutoCompact {
  auto_compact(OpenAIResponses { model: "gpt-5", api_key: env.OPENAI_API_KEY, compact_threshold: null }, initial)
}

// Anthropic — summarize-style compaction (compact_20260112). Note `edits` is set
// for the normal-turn pruning path, but `.compact`/checkpoint uses the summary
// primitive. Same combinator, different provider.
function make_anthropic_session(initial: WindowItems) -> AutoCompact {
  auto_compact(
    Anthropic {
      model: "claude-sonnet-4-5",
      api_key: env.ANTHROPIC_API_KEY,
      max_tokens: 1024,
      edits: [
        ContextEdit { strategy: "clear_thinking_20251015", trigger_tokens: null, keep: 2 },
        ContextEdit { strategy: "clear_tool_uses_20250919", trigger_tokens: 50000, keep: 3 },
      ],
    },
    initial,
  )
}

// Gemini — no endpoint, so the provider summarizes via its own `call`. The app
// can't tell the difference at this layer: same combinator, same driver.
function make_gemini_session(initial: WindowItems) -> AutoCompact {
  auto_compact(GeminiSummary { model: "gemini-2.5-pro", api_key: env.GEMINI_API_KEY, keep_last: 6 }, initial)
}

// THE portable driver. Append turns; when a milestone lands, checkpoint. Identical
// body for all three providers — only the `session` factory differs.
function run_until_milestone(session: AutoCompact, turns: WindowItems[]) -> Compacted?
    throws baml.ExtendUnknownError<baml.errors.CompactionError> {
  let last: Compacted? = null;
  for (let turn in turns) {
    session.append_turn(turn);
    if (is_milestone(turn)) {
      let result = session.checkpoint();           // shrink + adopt verbatim
      if (result.busts_cache_prefix) { log_cache_miss(result.before_tokens, result.after_tokens); }
      last = result;
    }
  }
  last
}


// -----------------------------------------------------------------------------
// (4) WHERE PORTABILITY LEAKS — Anthropic's prune-in-place editing is a DIFFERENT
// shape. It is not "compact a held window"; it is "run a normal turn with edits
// attached and read what the server cleared." So it needs its own companion that
// `match`es `ContextEditing`, NOT `Compaction`. OpenAI and Gemini do not implement
// `ContextEditing`, so this companion is honestly unavailable for them.
//
// This is the leak the proposal predicts: two genuinely different mechanisms can't
// share one method signature, so they become two capabilities and the app picks.
// -----------------------------------------------------------------------------

client AnthropicEditor() {
  Anthropic {
    model: "claude-sonnet-4-5",
    api_key: env.ANTHROPIC_API_KEY,
    max_tokens: 1024,
    edits: [
      ContextEdit { strategy: "clear_thinking_20251015", trigger_tokens: null, keep: 2 },
      ContextEdit { strategy: "clear_tool_uses_20250919", trigger_tokens: 50000, keep: 3 },
    ],
  }
}

// prune-as-you-go: each call runs a turn and reports what the server cleared.
function EditDownTo(window: WindowItems) -> Compacted
    throws baml.ExtendUnknownError<baml.errors.CompactionError> {
  let p = AnthropicEditor();
  match (p) {
    let e: ContextEditing => {
      let w = anthropic_window_of(p, window);      // need the concrete window for edit()
      let result = e.edit(w);
      for (let removed in result.removed) {
        log_edit(removed.strategy, removed.cleared_items);   // applied_edits report
      }
      result
    },
    _ => throw baml.errors.Unsupported { message: "client does not support context editing" },
  }
}


// -----------------------------------------------------------------------------
// app-side host helpers / sinks.
// -----------------------------------------------------------------------------

function is_milestone(turn: WindowItems) -> bool { $rust_io_function }       // e.g. a sub-task finished
function log_cache_miss(before: int, after: int) -> null { $rust_io_function }
function log_edit(strategy: string, n: int) -> null { $rust_io_function }
function report_compaction_failure(message: string[]) -> null { $rust_io_function }
function anthropic_window_of(p: Provider, items: WindowItems) -> AnthropicWindow { $rust_io_function }
