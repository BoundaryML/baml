// scenario 17 — history & sessions: ../../../../../llm-provider/ideas/scenarios/17-history-and-sessions/implement.baml
//   implementation.baml — LIBRARY/std-lib-author code (custom shapes only).
//
// The design splits the memory problem into TWO models that the real surface
// already carries natively — so almost nothing here is net-new:
//
//   A. CLIENT-HELD history (design's transcript-IS-the-memory). The app threads a
//      native `baml.ai.ChatMessage[]` and resends the WHOLE array every turn,
//      appending BOTH the user turn AND the assistant turn. The real primitive is
//      `HttpProvider.call_messages(messages)` — the design's `Turn`/`Block` grammar
//      collapses to the shipped `ChatMessage`/`MessagePart` (roles + text/media
//      parts), and the per-provider wire divergence (OpenAI in-array system vs
//      Anthropic top-level `system`) lives natively inside each provider's
//      build_request (scenario 01). So this half needs only an OFFLINE fixture whose
//      reply REFLECTS the transcript it received, to prove the array actually
//      carries state across turns (and that forgetting the assistant turn loses it).
//
//   B. PROVIDER-HELD sessions (design's `Session` + pluggable store). The real
//      surface is the `baml.ai.Conversational` capability (`chat<T>(prompt, session)`)
//      over the opaque `baml.ai.Session` handle: the SERVER owns the transcript, the
//      client threads only the handle. The design's 4-method `SessionStore` and the
//      `Windowed`/`Summarizing` history-rewriting wrappers are the persistence host
//      surface — BLOCKED P8 (no durable store host op yet), so this half is realized
//      as shape + negotiation + Unsupported against a scripted fake.
//
// The design's client-held `Windowed`/`Summarizing` combinators become, on the
// real surface, plain client-side `ChatMessage[]` rewriting before `call_messages`
// (windowing = an array slice; summarizing = an ORDINARY LLM function writing the
// synopsis — the design's "the summarizer is itself a Provider", preferred here as
// an LLM function per the directive). Those helpers live here; the driver + LLM
// function live in usage.baml.
//
// New top-level decls carry the `s17_` prefix (all scenario dirs share ONE namespace).


// ── A. Client-held history: a transcript-REFLECTING offline provider ──────────
//
// common/fakes.baml's EchoProvider returns a FIXED reply regardless of input, so
// it can't prove threading. s17_TranscriptEcho instead reflects the transcript it
// was handed: its reply is the count of messages plus every user turn's text,
// joined. So a properly-threaded turn 2 sees turn 1 (its text appears in the
// reply); a turn 2 that FORGOT to re-append does not. This is the whole
// client-held lesson, made assertable offline.
class s17_TranscriptEcho {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        // Required abstract members (no default) — stubbed; call_messages_with is
        // overridden below so the whole call/call_messages surface routes through it.
        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcript echo has no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["transcript echo does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["transcript echo parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        // THE reflecting primitive: reply = "n=<count>;<user texts joined>". Because
        // the reply is derived from the messages received, it changes when the app
        // threads the transcript vs when it forgets to.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let reflected: string =
                "n=" + messages.length().to_string() + ";" + s17_user_texts(messages);
            let value: T = baml.sap.parse<T>(reflected) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["transcript echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}

// Pull every user message's text out of a transcript, joined with " | ". Uses the
// house-style null-elimination match on the `text: string?` part (a `_` arm would
// break the media runtime type-test — see baml_gotchas.md).
function s17_user_texts(messages: baml.ai.ChatMessage[]) -> string {
    let texts: string[] = [];
    for (let m in messages) {
        if (m.role == "user") {
            for (let p in m.parts) {
                match (p.text) {
                    null => {},
                    let t: string => { texts.push(t); },
                }
            }
        }
    }
    texts.join(" | ")
}

// Null-safe accessors for the assertions (a bare `msgs.at(i).field` is an E0001 —
// `.at` is nullable). House-style null-elimination match.
function s17_role_at(msgs: baml.ai.ChatMessage[], i: int) -> string {
    match (msgs.at(i)) {
        null => "",
        let m: baml.ai.ChatMessage => m.role,
    }
}
function s17_head_text(msgs: baml.ai.ChatMessage[]) -> string {
    match (msgs.at(0)) {
        null => "",
        let m: baml.ai.ChatMessage => {
            match (m.parts.at(0)) {
                null => "",
                let p: baml.ai.MessagePart => {
                    match (p.text) {
                        null => "",
                        let t: string => t,
                    }
                },
            }
        },
    }
}

// Windowing (design §6, client-held): keep only the last `keep` turns, ALWAYS
// preserving a leading system message so the persona survives the trim. Lossy-by-
// forgetting; a pure array rewrite before `call_messages`.
function s17_window_tail(history: baml.ai.ChatMessage[], keep: int) -> baml.ai.ChatMessage[] {
    let head: baml.ai.ChatMessage[] = [];
    let rest: baml.ai.ChatMessage[] = history;
    match (history.at(0)) {
        null => {},
        let first: baml.ai.ChatMessage => {
            if (first.role == "system") {
                head.push(first);
                rest = history.slice(1, history.length());
            }
        },
    }
    let diff: int = rest.length() - keep;
    let start: int = if (diff > 0) { diff } else { 0 };
    head.concat(rest.slice(start, rest.length()))
}

// Summarizing (design §6, client-held): replace the turns older than `keep` with a
// single synopsis system message, then keep the tail. Lossy-by-distortion. The
// synopsis text is produced by an ORDINARY LLM function (s17_Summarize in
// usage.baml) — the design's "the summarizer is itself a Provider", preferred as an
// LLM function per the directive. This helper just folds the computed synopsis in.
function s17_fold_with_synopsis(
    history: baml.ai.ChatMessage[],
    keep: int,
    synopsis: string,
) -> baml.ai.ChatMessage[] {
    let n: int = history.length();
    if (n <= keep) {
        return history;
    }
    let folded: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.system("Earlier conversation summary: " + synopsis),
    ];
    folded.concat(history.slice(n - keep, n))
}


// ── B. Provider-held sessions: a scripted Conversational fake ─────────────────
//
// The design's Session + SessionStore, on the real surface, is the `Conversational`
// capability over the opaque `baml.ai.Session` handle. The SERVER owns the
// transcript; the client threads only the handle. Real persistence (the design's
// load/save/append/clear store) is the P8 host surface — BLOCKED — so this fake
// only proves the negotiation + handle threading: its reply is derived from the
// session id (the handle) it was given, demonstrating the client holds no array.
//
// BLOCKED: P8 — durable session store. The design's 4-method SessionStore
// (load/save/append/clear over in-process/Redis/Postgres) and cross-call
// server-side accumulation have no host op yet; this fake threads the handle but
// cannot persist a growing transcript across calls.
class s17_Sessions {
    implements baml.ai.Provider {}

    implements baml.ai.Conversational {
        function chat<T>(self, prompt: string, session: baml.ai.Session) -> T
            throws baml.errors.UnknownError {
            // reply reflects BOTH the handle (server-owned identity) and the new
            // prompt — the client never sent a transcript, only `session`.
            baml.sap.parse<T>("[session " + session._id + "] " + prompt) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["session chat parse failed"] },
            }
        }
    }
}
