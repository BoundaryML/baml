// scenario 21 — memory (cross-conversation, long-term): ../../../../../llm-provider/ideas/scenarios/21-memory/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// Two tracks, mirroring the design (README + implement.baml + evaluation.md):
//
//   (a) PROVIDER-HELD memory — the `baml.ai.MemoryStore` capability (recall/remember),
//       negotiated over the existential. Shape + negotiation only; durable
//       persistence is BLOCKED P8 (scripted fake s21_ScriptedMemory).
//
//   (b) CLIENT-HELD memory — the design-true, fully-realizable pipeline (the design's
//       "the extractor/decider are just Providers the memory layer consumes"):
//         extract (llm fn, structured output) -> store (plain BAML list) ->
//         recall (inject stored memories into the next call's prompt) -> answer (llm fn).
//       Offline: EchoProvider scripts the extract; s21_PromptEcho reflects the injected
//       block so recall is assertable. Live: a real extract->recall round trip where a
//       fact stated in one conversation is recalled to answer a later question.
//
// New app decls carry the `s21_` prefix (all scenario dirs share ONE namespace).


// ── Clients ───────────────────────────────────────────────────────────────────
// A dead default so every offline test passes an explicit deterministic client=.
client<llm> s21_DeadChat {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}


// ── (b) The two LLM functions the memory pipeline is built from ────────────────

// EXTRACTOR — conversation in, structured memory items out. An ordinary LLM function
// (design: "the extractor is itself a Provider"). Its client is swapped at the call
// site: an echo offline, a real model live.
function s21_ExtractMemories(conversation: string) -> s21_Memories {
    client s21_DeadChat
    prompt #"
        {{ _.role("system") }}
        Extract durable, third-person facts worth remembering long-term about the user
        from the conversation below. Each fact is a subject and a short declarative
        statement (e.g. subject "user", fact "is vegetarian"). Skip small talk.

        {{ _.role("user") }}
        {{ conversation }}
    "#
}

// ANSWERER — recall in action: the stored memories are injected into the system
// position, and a fresh question is answered WITH them in context. This is the whole
// "recall = inject relevant memories into the next call's prompt" step.
function s21_AnswerWithMemory(memories: string, question: string) -> string {
    client s21_DeadChat
    prompt #"
        {{ _.role("system") }}
        You are a helpful assistant. Use what you remember about this user to answer.
        What you remember about this user:
        {{ memories }}

        {{ _.role("user") }}
        {{ question }}
    "#
}

// The pipeline driver (app code): extract -> store -> recall -> answer, with an
// observability log at every meaningful point. The extractor and answerer are handed
// in as providers so the same body runs offline (fakes) and live (real models).
function s21_remember_then_recall(
    conversation: string,
    question: string,
    extractor: baml.ai.Provider,
    answerer: baml.ai.Provider,
) -> string throws baml.errors.CallError | baml.errors.UnknownError
    | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt | baml.errors.StreamError {
    // (1) EXTRACT — conversation -> structured memory items.
    let extracted: s21_Memories = s21_ExtractMemories(conversation, client = extractor);
    log.info("21 extracted memories: " + s21_render_memories(extracted.items));

    // (2) STORE — an app-side plain BAML list (no host; the app owns the store).
    let store: s21_MemoryItem[] = [];
    for (let it in extracted.items) {
        store.push(it);
    }

    // (3) RECALL — inject the stored memories into the next call's prompt.
    let injected: string = s21_render_memories(store);
    log.info("21 injected context: " + injected);

    // (4) ANSWER — a fresh call that can only answer because the memories are in scope.
    let answer: string = s21_AnswerWithMemory(injected, question, client = answerer);
    log.info("21 recall answer: " + answer);
    answer
}


// ============================================================================
// OFFLINE TESTS (deterministic, no network / no keys)
// ============================================================================

// ── (b) client-held pipeline ──────────────────────────────────────────────────

test "21: the extractor pulls structured memory items from a conversation" {
    let mem = s21_ExtractMemories(
        "User: I'm vegetarian and I have a dog named Bella.\nAssistant: Noted!",
        client = EchoProvider {
            reply: "{\"items\":[{\"subject\":\"user\",\"fact\":\"is vegetarian\"},{\"subject\":\"user\",\"fact\":\"has a dog named Bella\"}]}",
        },
    ) catch (e) {
        _ => s21_Memories { items: [] },
    };
    assert.equal(mem.items.length(), 2)
    assert.equal(mem.items[0].fact, "is vegetarian")
    assert.is_true(mem.items[1].fact.includes("Bella"))
}

test "21: rendering the store folds the memories into a prompt-injectable block" {
    let block = s21_render_memories([
        s21_MemoryItem { subject: "user", fact: "is vegetarian" },
        s21_MemoryItem { subject: "user", fact: "has a dog named Bella" },
    ]);
    assert.is_true(block.includes("is vegetarian"))
    assert.is_true(block.includes("Bella"))
}

test "21: extract -> store -> recall threads a fact into the answer prompt" {
    // s21_PromptEcho reflects the whole prompt it received, so the injected memory
    // block appearing in the answer PROVES recall reached the model — a fixed-reply
    // echo could not show this.
    let answer = s21_remember_then_recall(
        "User: I'm vegetarian and I have a dog named Bella.\nAssistant: Noted!",
        "What should I cook, and what is my pet's name?",
        EchoProvider {
            reply: "{\"items\":[{\"subject\":\"user\",\"fact\":\"is vegetarian\"},{\"subject\":\"user\",\"fact\":\"has a dog named Bella\"}]}",
        },
        s21_PromptEcho {},
    ) catch (e) {
        _ => "pipeline failed",
    };
    assert.is_true(answer.includes("vegetarian"))
    assert.is_true(answer.includes("Bella"))
}

test "21: an empty extraction recalls nothing (the no-memories floor)" {
    let answer = s21_remember_then_recall(
        "User: hello.\nAssistant: hi!",
        "anything?",
        EchoProvider { reply: "{\"items\":[]}" },
        s21_PromptEcho {},
    ) catch (e) {
        _ => "pipeline failed",
    };
    assert.is_true(answer.includes("(no memories yet)"))
}

// ── (a) provider-held MemoryStore capability: shape + negotiation ──────────────

test "21: recall surfaces scored hits for a query (provider-held capability)" {
    let p: baml.ai.Provider = s21_ScriptedMemory { seed: "user likes turtles" };
    let none: baml.ai.MemoryHit[] = [];
    let hits: baml.ai.MemoryHit[] = match (p) {
        let m: baml.ai.MemoryStore => m.recall("pets", 3) catch (e) { _ => none },
        _ => none,
    };
    assert.equal(hits.length(), 1)
    assert.is_true(hits[0].score > 0.5)
    assert.is_true(hits[0].text.includes("turtles"))
    assert.is_true(hits[0].text.includes("pets"))   // query threaded through
}

test "21: remember is negotiated over the existential (persist no-op — BLOCKED P8)" {
    let p: baml.ai.Provider = s21_ScriptedMemory { seed: "seed" };
    let ok: bool = match (p) {
        let m: baml.ai.MemoryStore => {
            m.remember("user moved to Lisbon") catch (e) { _ => {} };
            true
        },
        _ => false,
    };
    assert.is_true(ok)
}

test "21: a non-memory client is simply not a MemoryStore (negotiation miss)" {
    let p: baml.ai.Provider = NoCapProvider {};
    let label: string = match (p) {
        let m: baml.ai.MemoryStore => "has memory",
        _ => "no memory",
    };
    assert.equal(label, "no memory")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ────
// The client-held claim against a REAL model: a fact stated in ONE conversation is
// pulled by the extractor, stored, injected, and RECALLED to answer a later question
// that needs it — the full extract->recall round trip end to end.
testset "integ-test-21-memory" {
    test "21 live: a stated fact is extracted, stored, and recalled to answer later" {
        let extractor: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let answerer: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let answer = s21_remember_then_recall(
            "User: My favorite color is teal and I live in Lisbon.\nAssistant: Got it, thanks!",
            "Where do I live? Reply with just the city name.",
            extractor,
            answerer,
        ) catch (e) {
            _ => "live failed",
        };
        // the answerer only knows Lisbon because the extractor pulled it into the
        // store and recall injected it into this fresh call's prompt.
        assert.is_true(answer.includes("Lisbon") || answer.includes("lisbon"))
    }
}
