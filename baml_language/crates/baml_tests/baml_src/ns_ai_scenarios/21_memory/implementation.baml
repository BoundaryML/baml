// scenario 21 — memory (cross-conversation, long-term): ../../../../../llm-provider/ideas/scenarios/21-memory/README.md
//   implementation.baml — LIBRARY/std-lib-author code (custom shapes only).
//
// The design's headline: "memory is NOT a Provider." A provider produces a typed
// answer; a memory layer stores and recalls FACTS. So the design has two tracks,
// and the real surface carries both:
//
//   (a) PROVIDER-HELD memory — the shipped `baml.ai.MemoryStore` capability
//       (recall/remember), negotiated over the existential exactly like every other
//       capability. Real durable persistence (the design's Qdrant "disk" + the
//       extract→decide→persist curation pipeline behind a stateful store) is the P8
//       host surface — BLOCKED — so this track is realized as shape + negotiation
//       against a scripted fake (s21_ScriptedMemory below), with `remember` a no-op.
//
//   (b) CLIENT-HELD memory — the design-true pipeline that IS fully realizable on the
//       real surface with NO new host op, and is the payoff the design points at
//       ("the extractor/decider are just Providers the memory layer consumes"):
//         1. an EXTRACTOR llm function: conversation -> structured memory items,
//         2. an app-side STORE: a plain BAML `s21_MemoryItem[]` list (no host),
//         3. RECALL = injecting the stored memories into the next call's prompt,
//            answered by a second llm function.
//       Offline: EchoProvider scripts the extract, and a transcript-reflecting fake
//       (s21_PromptEcho) proves the injected memories actually reach the answer
//       prompt. Live: a real extract->recall round trip (usage.baml integ tier).
//
// New top-level decls carry the `s21_` prefix (all scenario dirs share ONE namespace).


// ── (b) Client-held memory: the structured shapes ─────────────────────────────

/// One durable fact worth remembering across conversations. Two fields (subject +
/// fact) so SAP actually validates the shape (a single-string-field class SAP-parses
/// from almost anything — see baml_gotchas.md).
class s21_MemoryItem {
    subject: string,
    fact: string,
}

/// The extractor's structured output: the list of facts pulled from a conversation.
/// A container class (not a bare top-level array) — the well-proven structured-output
/// shape (scenario 02).
class s21_Memories {
    items: s21_MemoryItem[],
}

// Fold the app-side store into a prompt-injectable block. This is the whole "recall"
// step: rendered memories are spliced into the next call's system prompt. Pure BAML,
// no host — the app owns the store and the injection.
function s21_render_memories(items: s21_MemoryItem[]) -> string {
    if (items.length() == 0) {
        return "(no memories yet)";
    }
    let lines: string[] = [];
    for (let it in items) {
        lines.push("- " + it.subject + ": " + it.fact);
    }
    lines.join("\n")
}

// Pull every message's text (any role), concatenated. Used by the reflecting fake so
// an offline test can see the injected memory block that landed in the prompt. Uses
// the house-style null-elimination match on the `text: string?` part (a `_` arm would
// break the media runtime type-test — see baml_gotchas.md).
function s21_all_texts(messages: baml.ai.ChatMessage[]) -> string {
    let texts: string[] = [];
    for (let m in messages) {
        for (let p in m.parts) {
            match (p.text) {
                null => {},
                let t: string => { texts.push(t); },
            }
        }
    }
    texts.join(" ")
}

// A transcript-REFLECTING offline provider. common/fakes.baml's EchoProvider returns
// a FIXED reply regardless of input, so it can't prove the injected memories reached
// the prompt. s21_PromptEcho instead reflects the concatenated prompt text it was
// handed — so a recall step that injected the stored facts sees those facts appear in
// the answer. This is the whole client-held recall claim, made assertable offline.
class s21_PromptEcho {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["prompt echo has no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["prompt echo does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["prompt echo parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        // THE reflecting primitive: reply = the whole rendered prompt text. Because
        // the reply is derived from the messages received, an injected memory block
        // shows up verbatim in the answer.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let reflected: string = s21_all_texts(messages);
            let value: T = baml.sap.parse<T>(reflected) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["prompt echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// ── (a) Provider-held memory: a scripted MemoryStore fake ─────────────────────
//
// The shipped `baml.ai.MemoryStore` capability (recall/remember) — negotiated over
// the existential like every other capability. BLOCKED: P8 — durable stores. Real
// persistence (a vector "disk" + curated extract→decide→persist behind a stateful
// store) has no host op yet, so this fake scripts recall from a seed and makes
// `remember` a no-op (it cannot persist across calls without the P8 surface).
class s21_ScriptedMemory {
    seed: string,

    implements baml.ai.Provider {}

    implements baml.ai.MemoryStore {
        // recall: return the seeded fact as a scored hit, echoing the query so a test
        // can see the query threaded through. Real relevance search is the P8 disk.
        function recall(self, query: string, k: int) -> baml.ai.MemoryHit[]
            throws baml.errors.UnknownError {
            [baml.ai.MemoryHit { text: self.seed + " (re: " + query + ")", score: 0.9 }]
        }
        // remember: no-op. BLOCKED P8 — no durable store to persist into; a live
        // implementation would upsert the fact into the vector disk here.
        function remember(self, fact: string) -> null
            throws baml.errors.UnknownError {
            null
        }
    }
}
