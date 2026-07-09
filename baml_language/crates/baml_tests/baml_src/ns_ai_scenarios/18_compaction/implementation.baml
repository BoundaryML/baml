// scenario 18 — compaction: ../../../../../llm-provider/ideas/scenarios/18-compaction/implement.baml
//   implementation.baml — the custom shapes this scenario needs.
//
// The design's implement.baml hand-rolled a whole provider-side model: a
// `Compaction` capability with an opaque `Window`, three diverging providers
// (OpenAI replacement / Anthropic context-editing / Gemini roll-your-own summary),
// a `ContextEditing` sibling capability, an `AutoCompact` combinator carrying the
// window between turns, a `Compacted` result with cache/token metadata, and a
// net-new `baml.errors.CompactionError` channel.
//
// On the REAL surface that splits cleanly into TWO threads, each realized here:
//
//   (1) The client-side, portable mechanism — summarize-and-replace over the
//       native `baml.ai.ChatMessage[]` history. THIS is the design-true spelling
//       of Gemini's "roll-your-own summary": the summarizer is an ordinary LLM
//       `function` (usage.baml `s18_SummarizeHead`), and window management is
//       plain BAML over `ChatMessage[]` (`s18_compact_history` below). It needs
//       no net-new provider surface at all — it composes an LLM function with
//       array slicing. This is what actually runs, offline and live.
//
//   (2) The provider-held `Compaction` capability — real stdlib
//       (`baml.ai.Compaction`: `window_of(session) -> Window`,
//       `compact(window) -> Window`, opaque `Window`/`Session` handles). Its
//       persistence/host surface is BLOCKED P8, so only the SHAPE + runtime
//       NEGOTIATION are exercised, against the scripted fake below. The design's
//       `Compacted { output, busts_cache_prefix, before/after_tokens, removed }`
//       metadata, the `ContextEditing` sibling, and the `AutoCompact` stateful
//       combinator have no real surface yet and are recorded as divergences.

// -----------------------------------------------------------------------------
// (1) Window management — plain BAML over the native ChatMessage[] history.
// The summarize-and-replace primitive: keep the last `keep_last` turns verbatim,
// fold everything older into a single system "summary" message produced by the
// `s18_SummarizeHead` LLM function (usage.baml). It only touches array slicing +
// one LLM call, so it is portable across ANY provider handed in as `client`.
// -----------------------------------------------------------------------------

/// Flatten a message's text parts (media parts contribute nothing to the synopsis).
function s18_text_of(m: baml.ai.ChatMessage) -> string throws never {
    let texts: string[] = m.parts.map((p: baml.ai.MessagePart) -> string throws never {
        p.text ?? ""
    });
    texts.join("")
}

/// Render a run of turns into the plain-text transcript the summarizer reads.
function s18_render_head(head: baml.ai.ChatMessage[]) -> string throws never {
    let lines: string[] = head.map((m: baml.ai.ChatMessage) -> string throws never {
        m.role + ": " + s18_text_of(m)
    });
    lines.join("\n")
}

/// Summarize-and-replace: shrink `history` to [one summary message] ++ the last
/// `keep_last` turns. Histories at/under the threshold are returned untouched (no
/// LLM call, nothing to compact). The summarizer is the `s18_SummarizeHead` LLM
/// function, driven through whatever provider the caller wires in via `client`.
/// The LLM call's render/negotiation errors are re-homed onto the CallError /
/// UnknownError channel so this stays a clean two-channel surface.
function s18_compact_history(
    history: baml.ai.ChatMessage[],
    keep_last: int,
    client: baml.ai.Provider,
) -> baml.ai.ChatMessage[] throws baml.errors.CallError | baml.errors.UnknownError {
    let n = history.length();
    if (n <= keep_last) { return history; }
    let head = history.slice(0, n - keep_last);
    let tail = history.slice(n - keep_last, n);
    let rendered = s18_render_head(head);
    let synopsis = s18_SummarizeHead(rendered, client = client) catch (e) {
        let c: baml.errors.CallError => throw c,           // negotiation/transport passes through
        _ => throw baml.errors.UnknownError { data: e, message: ["summarize-head failed"] },
    };
    let summary_msg = baml.ai.ChatMessage.system("Summary of earlier conversation: " + synopsis);
    [summary_msg].concat(tail)
}

// -----------------------------------------------------------------------------
// (2) Provider-held Compaction capability — SHAPE + NEGOTIATION only (P8-blocked).
// The real stdlib `baml.ai.Compaction` round-trips an OPAQUE `Window` handle: the
// caller obtains it from a `Session`, hands it to `compact`, and passes the result
// forward verbatim WITHOUT ever decoding it (the design's "lossy machine state").
// Persistence is not built, so we script the handle transitions to prove the
// interface + the `match (provider) { let c: Compaction => … }` negotiation.
// The same fake also implements `Branching` (scenario 19 has no dir of its own).
// -----------------------------------------------------------------------------

/// Offline Compaction + Branching provider — scripts the opaque handle transitions.
class ScriptedCompaction {
    implements baml.ai.Provider {}

    implements baml.ai.Compaction {
        function window_of(self, session: baml.ai.Session) -> baml.ai.Window
            throws baml.errors.UnknownError {
            baml.ai.Window { _data: "w:" + session._id }
        }
        function compact(self, window: baml.ai.Window) -> baml.ai.Window
            throws baml.errors.UnknownError {
            // opaque replacement state — the caller forwards it verbatim
            baml.ai.Window { _data: window._data + ":compacted" }
        }
    }

    implements baml.ai.Branching {
        function fork(self, session: baml.ai.Session) -> baml.ai.Session
            throws baml.errors.UnknownError {
            baml.ai.Session { _id: session._id + "-fork" }
        }
    }
}
