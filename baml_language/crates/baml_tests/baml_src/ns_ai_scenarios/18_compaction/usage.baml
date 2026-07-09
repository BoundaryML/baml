// scenario 18 — compaction: ../../../../../llm-provider/ideas/scenarios/18-compaction/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface.
//
// A long run accumulates old turns that crowd out the state the model needs.
// Compaction shrinks the live window while preserving forward-looking state. On
// the real surface the portable, provider-independent spelling is CLIENT-SIDE
// summarize-and-replace over the native `baml.ai.ChatMessage[]` history:
//   - the summarizer IS an ordinary LLM `function` (`s18_SummarizeHead`),
//   - window management is plain BAML (`s18_compact_history`, implementation.baml):
//     keep the last N turns verbatim, fold the rest into one summary message.
// Offline the summarizer runs against an EchoProvider (from common/fakes.baml) that
// returns the synopsis deterministically — no network, no keys. The live tier
// swaps in a real OpenAI client with the SAME function body.
//
// The provider-held `baml.ai.Compaction` capability (opaque Window round-trip) is
// the OTHER thread: its persistence is BLOCKED P8, so only shape + negotiation are
// exercised, against the scripted fake in implementation.baml.

// --- The summarizer: an LLM function. ---------------------------------------
// Condenses the head of the transcript into a forward-looking synopsis. The body
// is provider-agnostic; the offline tests override `client =` with an EchoProvider,
// the live tier with a native OpenAi. Defaults to a dead client so every offline
// test must pass an explicit, deterministic override.
client<llm> DeadS18 {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s18_SummarizeHead(head: string) -> string {
    client DeadS18
    prompt #"
        {{ _.role("system") }}
        Condense the earlier conversation into a compact synopsis that preserves
        forward-looking state — decisions made, open questions, and facts still in
        play. Drop pleasantries and superseded detail. Be terse.

        {{ _.role("user") }}
        {{ head }}
    "#
}


// --- (1) Client-side summarize-and-replace. ---------------------------------
// The core behavior: an over-long history shrinks to [summary] ++ recent turns,
// the recent turns preserved verbatim. The summary carries the echoed synopsis.
test "18: summarize-and-replace shrinks the head, keeps recent turns verbatim" {
    let history: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.user("plan the offsite"),
        baml.ai.ChatMessage.assistant("booked the venue"),
        baml.ai.ChatMessage.user("sort out catering"),
        baml.ai.ChatMessage.assistant("ordered lunch for 20"),
        baml.ai.ChatMessage.user("what's left to do?"),
    ];
    let compacted: baml.ai.ChatMessage[] =
        s18_compact_history(history, 2, EchoProvider { reply: "Offsite: venue + catering booked." }) catch (e) {
            _ => [],
        };
    // one summary message + the last two turns kept verbatim
    assert.equal(compacted.length(), 3)

    let first = compacted.at(0);
    let summary_text = match (first) { null => "none", let m: baml.ai.ChatMessage => s18_text_of(m) };
    assert.is_true(summary_text.includes("Offsite: venue + catering booked."))

    // the tail is untouched: last message is still the verbatim final user turn
    let last = compacted.at(2);
    let last_text = match (last) { null => "none", let m: baml.ai.ChatMessage => s18_text_of(m) };
    assert.equal(last_text, "what's left to do?")
}

// A history at/under the keep-last threshold has nothing to compact and is
// returned untouched — no LLM call is made (the EchoProvider reply is never read).
test "18: a short history under the keep-last threshold is left untouched" {
    let history: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.user("hi"),
        baml.ai.ChatMessage.assistant("hello"),
    ];
    let compacted: baml.ai.ChatMessage[] =
        s18_compact_history(history, 3, EchoProvider { reply: "unused" }) catch (e) {
            _ => [],
        };
    assert.equal(compacted.length(), 2)
    let last = compacted.at(1);
    let last_text = match (last) { null => "none", let m: baml.ai.ChatMessage => s18_text_of(m) };
    assert.equal(last_text, "hello")
}

// The summarizer is a real LLM function, so it NEGOTIATES: a client whose provider
// can neither call nor stream yields a typed Unsupported (on the CallError channel),
// re-homed by s18_compact_history — compaction fails cleanly instead of silently.
test "18: compaction fails cleanly when the summarizer client cannot call" {
    let history: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.user("a"),
        baml.ai.ChatMessage.assistant("b"),
        baml.ai.ChatMessage.user("c"),
    ];
    let label = s18_compact_history(history, 1, NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        let c: baml.errors.CallError   => "call-error",
        _                              => "other",
    };
    // Unsupported implements CallError, so either arm is honest; assert it's typed.
    assert.is_true(label == "unsupported" || label == "call-error")
}


// --- (2) Provider-held Compaction capability — shape + negotiation (P8). -----
// The opaque `Window` handle round-trips through `compact` WITHOUT the caller ever
// decoding it. Persistence is BLOCKED P8; the scripted fake proves the interface
// and the runtime capability negotiation over the existential Provider.
test "18: the provider-held Compaction window round-trips opaquely (shape only; P8)" {
    let p: baml.ai.Provider = ScriptedCompaction {};
    let label = match (p) {
        let c: baml.ai.Compaction => {
            let w = c.window_of(baml.ai.Session { _id: "s9" }) catch (e) { _ => baml.ai.Window { _data: "failed" } };
            let w2 = c.compact(w) catch (e) { _ => baml.ai.Window { _data: "failed" } };
            w2._data
        },
        _ => "no compaction",
    };
    assert.equal(label, "w:s9:compacted")
}

// scenario 19 (fork/branch) has no dir of its own; its negotiation lives here on
// the same fake.
test "19: forking a session yields a distinct handle" {
    let p: baml.ai.Provider = ScriptedCompaction {};
    let forked = match (p) {
        let b: baml.ai.Branching => {
            let f = b.fork(baml.ai.Session { _id: "root" }) catch (e) { _ => baml.ai.Session { _id: "failed" } };
            f._id
        },
        _ => "no branching",
    };
    assert.equal(forked, "root-fork")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The SAME summarize-and-replace body against a real OpenAI client: the summarizer
// actually condenses the head of a transcript, and the recent turns stay verbatim.
testset "integ-test-18-compaction" {
    test "18 live: summarize-and-replace over real OpenAI compaction" {
        let history: baml.ai.ChatMessage[] = [
            baml.ai.ChatMessage.user("We are debugging a refund bug. Reproduced on staging."),
            baml.ai.ChatMessage.assistant("Root cause: the refund webhook double-fires."),
            baml.ai.ChatMessage.user("We added an idempotency key. Fixed."),
            baml.ai.ChatMessage.assistant("Confirmed no duplicate refunds after the fix."),
            baml.ai.ChatMessage.user("Now: what's the next task?"),
        ];
        let client_p: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let compacted: baml.ai.ChatMessage[] =
            s18_compact_history(history, 1, client_p) catch (e) {
                _ => [],
            };
        // shrank to [summary] ++ the last turn
        assert.equal(compacted.length(), 2)
        let last = compacted.at(1);
        let last_text = match (last) { null => "none", let m: baml.ai.ChatMessage => s18_text_of(m) };
        assert.equal(last_text, "Now: what's the next task?")
        // the summary is non-empty machine-produced prose
        let first = compacted.at(0);
        let summary_text = match (first) { null => "none", let m: baml.ai.ChatMessage => s18_text_of(m) };
        assert.is_true(summary_text.length() > "Summary of earlier conversation: ".length())
    }
}
