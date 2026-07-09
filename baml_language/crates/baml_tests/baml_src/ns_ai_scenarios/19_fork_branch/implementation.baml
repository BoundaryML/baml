// scenario 19 — fork / branch: ../../../../../llm-provider/ideas/scenarios/19-fork-branch/implement.baml
//   implementation.baml — LIBRARY/std-lib-author code (custom shapes only).
//
// "Git for conversations": take a session at some point in its history and
// explore a different continuation without destroying the original. The design's
// implement.baml hand-rolled a whole provider-side model — a `Session` capability
// (open/turn/latest/head/cursor_at/transcript), a `Branching` capability
// (fork/copy_out), an opaque `Cursor`, a read-only `Transcript { complete }`
// view, a tree-shaped `NodeStore`, and two net-new error channels
// (`baml.errors.SessionError`/`BranchError`) — and made the three real-world
// encodings concrete (copy-the-prefix / point-at-parent / emergent-from-ids).
//
// On the REAL surface that splits into TWO design-true threads, each realized here:
//
//   (a) CLIENT-HELD branching — the design's "copy-the-prefix" (Claude Agent SDK):
//       a conversation IS a native `baml.ai.ChatMessage[]`, and a fork is just a
//       COPY of the prefix that continues independently. The original prefix is
//       never mutated — that is the whole point of branching. This needs NO
//       net-new provider surface: fork = `prefix.concat([...])` (a copy), a
//       branch runs via `HttpProvider.call_messages`, and "fork anywhere" is a
//       slice at an earlier index. Fully offline-testable AND live-provable.
//       Branches are explored CONCURRENTLY via spawn/await at the TOP LEVEL of a
//       function body (the spawn-in-closure deadlock gotcha forbids spawning
//       inside a `.map` closure).
//
//   (b) PROVIDER-HELD branching — the real stdlib `baml.ai.Branching` capability
//       (`fork(session) -> session` over the opaque `baml.ai.Session` handle).
//       Its durable store is BLOCKED P8, so only the SHAPE + runtime NEGOTIATION
//       are exercised, against the scripted fake below. The design's opaque
//       `Cursor`, the `Transcript { complete }` view, the `NodeStore` DAG, and the
//       `SessionError`/`BranchError` channels have no real surface yet and are
//       recorded as divergences.
//
// The LIVE tier proves the design's THIRD encoding — "emergent-from-ids": OpenAI
// Responses chains genuinely fork, because two `extend`s from the SAME
// `previous_response_id` are independent branches sharing a server-side prefix
// (usage.baml `testset`, on `baml.ai.OpenAiResponses`).
//
// New top-level decls carry the `s19_` prefix (all scenario dirs share ONE namespace).


// -----------------------------------------------------------------------------
// (a) Client-held branching — plain BAML over the native ChatMessage[] history.
// -----------------------------------------------------------------------------

/// One explored continuation: an id, the branch's OWN message array (a copy of the
/// shared prefix plus its divergent turns), and the model's reply on that branch.
class s19_Branch {
    id: string,
    messages: baml.ai.ChatMessage[],
    reply: string,
}

/// Flatten a message's text parts (media parts contribute nothing here).
function s19_text_of(m: baml.ai.ChatMessage) -> string throws never {
    let texts: string[] = m.parts.map((p: baml.ai.MessagePart) -> string throws never {
        p.text ?? ""
    });
    texts.join("")
}

/// FORK-AT-TAIL: copy the whole prefix and append one divergent user turn. `concat`
/// returns a NEW array, so the original `prefix` is left untouched — the branch and
/// the trunk share nothing mutable. This is the design's copy-the-prefix seam.
function s19_fork(prefix: baml.ai.ChatMessage[], continuation: string) -> baml.ai.ChatMessage[] throws never {
    prefix.concat([baml.ai.ChatMessage.user(continuation)])
}

/// FORK-ANYWHERE: copy the prefix only up to `at` (an index into the history) and
/// continue from there — even after the conversation moved on past `at`. The later
/// turns are dropped from the branch; the trunk still has them. `at` past the end
/// clamps to the full prefix (fork-at-tail).
function s19_fork_at(history: baml.ai.ChatMessage[], at: int, continuation: string) -> baml.ai.ChatMessage[] throws never {
    let n = history.length();
    let lo = if (at < 0) { 0 } else { at };
    let cut = if (lo > n) { n } else { lo };
    history.slice(0, cut).concat([baml.ai.ChatMessage.user(continuation)])
}

/// Run ONE branch: fork the shared prefix with `continuation`, drive the model over
/// the branch's own array, and package the reply. Logged so a live run reads as a
/// trace of which branch produced what. The model call negotiates the existential
/// `Provider` down to `HttpProvider` (request/response), exactly like every other
/// scenario; a provider that can't do request/response is a typed `Unsupported`.
function s19_run_branch(
    p: baml.ai.Provider,
    prefix: baml.ai.ChatMessage[],
    id: string,
    continuation: string,
) -> s19_Branch throws baml.errors.CallError | baml.errors.UnknownError {
    let messages = s19_fork(prefix, continuation);
    let reply: string = match (p) {
        let h: baml.ai.HttpProvider => h.call_messages<string>(messages),
        _ => throw baml.errors.Unsupported { message: "branch provider cannot do request/response" },
    };
    log.info("19 branch " + id + " reply: " + reply);
    s19_Branch { id: id, messages: messages, reply: reply }
}

/// Explore THREE continuations from the SAME shared prefix, CONCURRENTLY. The spawns
/// live at the TOP LEVEL of the function body (never inside a closure — that
/// deadlocks the VM per the gotchas), fanning the three branches out in parallel and
/// awaiting them back in. Each branch forked its own copy of `prefix`; the trunk is
/// untouched.
function s19_explore3(
    p: baml.ai.Provider,
    prefix: baml.ai.ChatMessage[],
    cont_a: string,
    cont_b: string,
    cont_c: string,
) -> s19_Branch[] throws baml.errors.CallError | baml.errors.UnknownError {
    let f_a = spawn { s19_run_branch(p, prefix, "a", cont_a) };
    let f_b = spawn { s19_run_branch(p, prefix, "b", cont_b) };
    let f_c = spawn { s19_run_branch(p, prefix, "c", cont_c) };
    let a = await f_a;
    let b = await f_b;
    let c = await f_c;
    [a, b, c]
}

/// Compare the explored branches and PICK A WINNER: the first branch whose reply
/// mentions `want` (a trivial, deterministic judge standing in for an LLM ranker),
/// falling back to the first branch. Logs the choice so a run documents itself.
function s19_pick_winner(branches: s19_Branch[], want: string) -> s19_Branch throws never {
    for (let b in branches) {
        if (b.reply.includes(want)) {
            log.info("19 winner: " + b.id + " (matched '" + want + "')");
            return b;
        }
    }
    match (branches.at(0)) {
        null => s19_Branch { id: "none", messages: [], reply: "" },
        let first: s19_Branch => {
            log.info("19 winner: " + first.id + " (fallback, no '" + want + "')");
            first
        },
    }
}


// -----------------------------------------------------------------------------
// Offline fake provider — a transcript-REFLECTING branch runner.
//
// common/fakes.baml's EchoProvider returns a FIXED reply regardless of input, so it
// can't show that two branches diverge OR that each carried the SHARED prefix.
// s19_ForkEcho instead reflects every USER turn's text, joined by " >> ". So a
// branch's reply contains BOTH the shared prefix's user turns AND its own divergent
// continuation — proving (offline, deterministically) that fork copied the prefix
// and that sibling branches differ only in their tail.
// -----------------------------------------------------------------------------

class s19_ForkEcho {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        // Required abstract members (no default) — stubbed; call_messages_with is
        // overridden below so the whole call/call_messages surface routes through it.
        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["fork echo has no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["fork echo does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["fork echo parse failed"] },
            }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let user_texts: string[] = [];
            for (let m in messages) {
                if (m.role == "user") {
                    user_texts.push(s19_text_of(m));
                }
            }
            let reflected = user_texts.join(" >> ");
            let value: T = baml.sap.parse<T>(reflected) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["fork echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }
}


// -----------------------------------------------------------------------------
// (b) Provider-held Branching capability — SHAPE + NEGOTIATION only (P8-blocked).
//
// The real stdlib `baml.ai.Branching` forks an OPAQUE `baml.ai.Session` handle:
// `fork(session) -> session`. The durable store behind the handle is not built, so
// we script the handle transition to prove the interface + the
// `match (provider) { let b: Branching => … }` negotiation. This is the design's
// point-at-parent / emergent-id encoding collapsed to the shipped one-method shape;
// the design's opaque `Cursor`, `Transcript { complete }` view, and `NodeStore` DAG
// are divergences (see the JSON report).
// -----------------------------------------------------------------------------

/// Offline Branching provider — scripts the opaque session-handle transition.
class s19_ForkableSession {
    implements baml.ai.Provider {}

    implements baml.ai.Branching {
        function fork(self, session: baml.ai.Session) -> baml.ai.Session
            throws baml.errors.UnknownError {
            // a fork mints a NEW, distinct handle sharing the parent's prefix; the
            // caller forwards it verbatim (the design's opaque Session move).
            baml.ai.Session { _id: session._id + "-fork" }
        }
    }
}
