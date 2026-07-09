// scenario 17 — history & sessions: ../../../../../llm-provider/ideas/scenarios/17-history-and-sessions/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design's core fact: "the transcript IS the memory." It splits into two models,
// realized here on the real surface exactly as the design splits them:
//
//   A. CLIENT-HELD history — the app threads a native `baml.ai.ChatMessage[]` and
//      resends the WHOLE array every turn, appending BOTH the user turn AND the
//      assistant turn. The real primitive is `HttpProvider.call_messages(messages)`.
//      The design's lesson here IS message threading, so the driver calls
//      `call_messages` over the threaded array directly (per the directive). Where an
//      LLM function fits — the summary writer — it IS an ordinary LLM function
//      (s17_Summarize). This half runs fully offline (s17_TranscriptEcho) and is
//      live-proven (the integ tier below, real OpenAI, remembers a name across turns).
//
//   B. PROVIDER-HELD sessions — the server owns the transcript; the client threads
//      only an opaque `baml.ai.Session` handle through the `baml.ai.Conversational`
//      capability (`chat<T>(prompt, session)`). BLOCKED P8: the design's pluggable
//      4-method SessionStore (durable load/save/append/clear) has no host op yet, so
//      this half is shape + negotiation + Unsupported against a scripted fake.
//
// The design's Windowed/Summarizing wrappers become client-side `ChatMessage[]`
// rewriting before the call (s17_window_tail / s17_fold_with_synopsis in
// implementation.baml). New app decls carry the `s17_` prefix (shared namespace).


// ── Clients ───────────────────────────────────────────────────────────────────
// A dead default so every offline test passes an explicit deterministic client=.
client<llm> s17_DeadChat {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// The one place an LLM function fits (design §6: "the summarizer is itself a
// Provider"): the synopsis writer is an ORDINARY LLM function. Its client is swapped
// at the call site — a cheap model live, an echo offline.
function s17_Summarize(transcript: string) -> string {
  client s17_DeadChat
  prompt #"
    {{ _.role("system") }}
    Summarize the earlier conversation in one terse line.

    {{ _.role("user") }}
    {{ transcript }}
  "#
}


// ── A. Client-held history: threading the transcript ───────────────────────────
//
// The two-turn driver, threaded CORRECTLY: it holds a `ChatMessage[]`, appends the
// user turn, calls, then appends BOTH the assistant turn AND the next user turn, and
// resends the whole array. Turn 2 therefore sees turn 1 — that's the memory.
function s17_two_turn(p: baml.ai.Provider, q1: string, q2: string) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  let history: baml.ai.ChatMessage[] = [baml.ai.ChatMessage.system("You are a terse assistant.")];
  match (p) {
    let h: baml.ai.HttpProvider => {
      // turn 1
      history.push(baml.ai.ChatMessage.user(q1));
      let r1: string = h.call_messages<string>(history);
      // append BOTH turns — forgetting the assistant turn is the classic bug (below).
      history.push(baml.ai.ChatMessage.assistant(r1));
      // turn 2 — resend the WHOLE array; "its"/"my name" resolves only because
      // turn 1 is still in it.
      history.push(baml.ai.ChatMessage.user(q2));
      h.call_messages<string>(history)
    },
    _ => throw baml.errors.Unsupported { message: "client's provider cannot do request/response" },
  }
}

// The SAME driver, with the design's named bug: it appends the user turn but FORGETS
// the assistant turn. Turn 2's array is [system, user1, user2] (3), not
// [system, user1, assistant1, user2] (4) — a corrupted, contradictory transcript.
function s17_two_turn_forgetful(p: baml.ai.Provider, q1: string, q2: string) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  let history: baml.ai.ChatMessage[] = [baml.ai.ChatMessage.system("You are a terse assistant.")];
  match (p) {
    let h: baml.ai.HttpProvider => {
      history.push(baml.ai.ChatMessage.user(q1));
      let r1: string = h.call_messages<string>(history);
      // BUG: the assistant turn is never appended.
      history.push(baml.ai.ChatMessage.user(q2));
      h.call_messages<string>(history)
    },
    _ => throw baml.errors.Unsupported { message: "client's provider cannot do request/response" },
  }
}


test "17: client-held history threads the whole transcript (user AND assistant)" {
    // s17_TranscriptEcho reflects the transcript it receives, so a correctly-threaded
    // turn 2 sees 4 messages AND turn 1's text — proof the array carried the state.
    let r = s17_two_turn(s17_TranscriptEcho {}, "capital of France?", "and its population?") catch (e) {
        _ => "call failed",
    };
    assert.is_true(r.includes("n=4"))
    assert.is_true(r.includes("capital of France?"))
    assert.is_true(r.includes("and its population?"))
}

test "17: forgetting the assistant turn corrupts the transcript (the classic bug)" {
    // Same driver minus the assistant append: turn 2 sees only 3 messages. The
    // contradiction the design warns about is now VISIBLE (n=3, not n=4).
    let r = s17_two_turn_forgetful(s17_TranscriptEcho {}, "capital of France?", "and its population?") catch (e) {
        _ => "call failed",
    };
    assert.is_true(r.includes("n=3"))
}

test "17: a non-request/response client cannot thread history (typed Unsupported)" {
    let r = s17_two_turn(NoCapProvider {}, "hi", "again") catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(r, "unsupported")
}


// ── A. Unbounded growth: windowing + summarizing (client-side rewriting) ───────

test "17: windowing keeps the tail and preserves the system persona" {
    // A grown transcript: system + 5 user/assistant pairs (11 messages).
    let history: baml.ai.ChatMessage[] = [baml.ai.ChatMessage.system("You are a terse assistant.")];
    for (let i in [1, 2, 3, 4, 5]) {
        history.push(baml.ai.ChatMessage.user("q" + i.to_string()));
        history.push(baml.ai.ChatMessage.assistant("a" + i.to_string()));
    }
    // keep the last 4 turns; the system message is always retained on top.
    let windowed = s17_window_tail(history, 4);
    assert.equal(windowed.length(), 5)          // 1 system + 4 tail
    assert.equal(s17_role_at(windowed, 0), "system")
    // the last window entry is the most-recent assistant answer.
    assert.equal(s17_user_texts(windowed), "q4 | q5")
}

test "17: summarizing folds the old turns into an LLM-written synopsis" {
    // The synopsis is written by an ordinary LLM function (design §6), its client
    // swapped for an echo offline.
    let old_transcript = "user asked the capital of France; assistant said Paris";
    let synopsis = s17_Summarize(old_transcript, client = EchoProvider { reply: "discussed the capital of France" }) catch (e) {
        _ => "summary failed",
    };
    // a grown transcript folded down to: [synopsis-system] + last 2 turns.
    let history: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.system("You are a terse assistant."),
        baml.ai.ChatMessage.user("older q1"),
        baml.ai.ChatMessage.assistant("older a1"),
        baml.ai.ChatMessage.user("recent q2"),
        baml.ai.ChatMessage.assistant("recent a2"),
    ];
    let folded = s17_fold_with_synopsis(history, 2, synopsis);
    assert.equal(folded.length(), 3)            // 1 synopsis system + 2 tail
    assert.equal(s17_role_at(folded, 0), "system")
    // the synopsis carries the compressed earlier context forward.
    let head_text = s17_head_text(folded);
    assert.is_true(head_text.includes("discussed the capital of France"))
}


// ── B. Provider-held sessions: the Conversational capability ───────────────────
//
// The server owns the transcript; the client threads only the opaque Session handle.
// Negotiated over the existential, exactly like every other capability.

test "17: provider-held sessions thread only the opaque session handle" {
    let p: baml.ai.Provider = s17_Sessions {};
    let reply = match (p) {
        let c: baml.ai.Conversational => {
            c.chat<string>("what did we discuss?", baml.ai.Session { _id: "sess-1" }) catch (e) {
                _ => "chat failed"
            }
        },
        _ => "not conversational",
    };
    // the reply reflects the handle (server-owned identity) — the client sent no array.
    assert.equal(reply, "[session sess-1] what did we discuss?")
}

test "17: a non-conversational client cannot hold a server session (Unsupported)" {
    let p: baml.ai.Provider = NoCapProvider {};
    let reply = match (p) {
        let c: baml.ai.Conversational => {
            c.chat<string>("hi", baml.ai.Session { _id: "sess-2" }) catch (e) { _ => "chat failed" }
        },
        _ => "not conversational",
    };
    assert.equal(reply, "not conversational")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ────
// The client-held threading claim against the REAL OpenAI API: tell the model a
// name on turn 1, ask for it on turn 2. It can only answer if turn 1 (user AND
// assistant) was threaded back — proof the array IS the memory, end to end.
testset "integ-test-17-history-sessions" {
    test "17 live: a threaded transcript remembers across turns" {
        let live: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let r = s17_two_turn(
            live,
            "My name is Fred. Reply with just: ok",
            "What is my name? Reply with just my name.",
        ) catch (e) {
            _ => "live call failed",
        };
        assert.is_true(r.includes("Fred") || r.includes("fred"))
    }
}
