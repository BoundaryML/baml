// scenario 20 — server-stored chains: ../../../../../llm-provider/ideas/scenarios/20-server-stored-chains/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design writes the same chat "server-stored-by-id" and shows what that ownership
// model buys (persist an id, not a transcript) and costs (you cannot migrate the chain).
// On the real surface the app never resends bytes: it holds a `baml.ai.ChainHandle` and
// calls `Chain.extend<T>(prompt, handle)`. This file exercises, per the design:
//
//   OFFLINE (s20_ScriptedChain, deterministic):
//     - extend threads ONLY the handle — the server (here the handle id) holds context.
//     - the D2 owner guard: extending a handle minted by a DIFFERENT provider fails typed
//       (the "state you cannot migrate" / cross-provider rejection).
//     - handle-as-data: a ChainHandle survives a string round-trip (persist an id, not
//       bytes) and rehydrates across a process restart to extend again.
//     - branching: two extends from ONE handle fork siblings that share the same root.
//
//   LIVE (testset "integ-test-20-server-chains", real OpenAI /v1/responses):
//     - memory recall: teach a fact in start_chain, extend recalls it — proof the server
//       held the context and only the handle was re-sent.
//     - fork: two extends from one handle both recall the shared root turn.
//
// Per the directive, extend<T> is a capability method used DIRECTLY (that IS the lesson
// of this scenario) rather than wrapped behind an LLM function.


// ── OFFLINE TIER ────────────────────────────────────────────────────────────────

test "20: extend threads only the handle (the server holds the context)" {
    // Mint a chain seeded with turn 1's context, then extend with a follow-up that
    // does NOT restate the fact. The reply still carries the seed — so it came from
    // the HANDLE, not the prompt. That is the server-stored-chain invariant.
    let chain = s20_ScriptedChain { owner: "scripted" };
    let handle = chain.start("the capital of France is Paris");

    let p: baml.ai.Provider = chain;
    let reply = match (p) {
        let c: baml.ai.Chain => c.extend<string>("and its population?", handle) catch (e) { _ => "extend failed" },
        _ => "no chain capability",
    };
    log.info("20 offline extend reply: " + reply);
    // the follow-up never mentioned Paris; recall proves the handle carried turn 1.
    assert.is_true(reply.includes("Paris"))
    assert.is_true(reply.includes("and its population?"))
}

test "20: a foreign-owner handle is refused, not silently re-driven (D2 owner guard)" {
    // A handle minted by OpenAI's Responses provider ("openai-responses"), smuggled into
    // the scripted provider. This is the design's cross-provider guard — state you cannot
    // migrate — and it must fail TYPED, not quietly run against foreign thread state.
    let chain = s20_ScriptedChain { owner: "scripted" };
    let foreign = baml.ai.ChainHandle { id: "resp_openai_xyz", owner: "openai-responses" };

    let p: baml.ai.Provider = chain;
    let outcome = match (p) {
        let c: baml.ai.Chain => c.extend<string>("continue", foreign) catch (e) {
            let u: baml.errors.UnknownError => "refused: " + u.message.join("; "),
        },
        _ => "no chain capability",
    };
    log.info("20 foreign-handle outcome: " + outcome);
    assert.is_true(outcome.starts_with("refused:"))
    assert.is_true(outcome.includes("openai-responses"))
}

test "20: a ChainHandle survives a string round-trip (persist an id, not bytes)" {
    // The durability win: the app persists ONLY the handle (a small string), never the
    // transcript. A later process rehydrates it and extends the SAME server-held chain.
    let chain = s20_ScriptedChain { owner: "scripted" };
    let handle = chain.start("secret code SPARROW-7");

    // ── serialize (what a real app would write to its store) ──
    let saved: string = baml.json.to_string<baml.ai.ChainHandle>(handle) catch (e) { _ => "SERIALIZE_FAILED" };
    log.info("20 persisted handle: " + saved);

    // ── rehydrate in a "later process" (SAP the string back into the handle) ──
    let rehydrated: baml.ai.ChainHandle = baml.sap.parse<baml.ai.ChainHandle>(saved) catch (e) {
        _ => baml.ai.ChainHandle { id: "PARSE_FAILED", owner: "" },
    };
    assert.equal(rehydrated.id, "secret code SPARROW-7")
    assert.equal(rehydrated.owner, "scripted")

    // the rehydrated handle still extends the chain — id, not bytes, was enough.
    let p: baml.ai.Provider = chain;
    let reply = match (p) {
        let c: baml.ai.Chain => c.extend<string>("what was the code?", rehydrated) catch (e) { _ => "extend failed" },
        _ => "no chain capability",
    };
    log.info("20 resumed reply: " + reply);
    assert.is_true(reply.includes("SPARROW-7"))
}

test "20: two extends from one handle fork siblings that share the root" {
    // Branching falls out of server-stored chains: extending the SAME prior handle twice
    // forks the thread. Both siblings see turn 1 (the shared root); neither sees the other.
    let chain = s20_ScriptedChain { owner: "scripted" };
    let root = chain.start("root fact: the sky is blue");

    let p: baml.ai.Provider = chain;
    let branchA = match (p) {
        let c: baml.ai.Chain => c.extend<string>("branch A question", root) catch (e) { _ => "A failed" },
        _ => "no chain",
    };
    let branchB = match (p) {
        let c: baml.ai.Chain => c.extend<string>("branch B question", root) catch (e) { _ => "B failed" },
        _ => "no chain",
    };
    log.info("20 fork A: " + branchA);
    log.info("20 fork B: " + branchB);
    // both branches recall the shared root ...
    assert.is_true(branchA.includes("the sky is blue"))
    assert.is_true(branchB.includes("the sky is blue"))
    // ... but each carries only its OWN follow-up (siblings, not a single thread).
    assert.is_true(branchA.includes("branch A question") && !branchA.includes("branch B"))
    assert.is_true(branchB.includes("branch B question") && !branchB.includes("branch A"))
}

test "20: a non-chain provider cannot hold a server-stored chain (typed negotiation)" {
    // Ownership is a provider property: a provider that is not a Chain has no server-held
    // thread, so the capability negotiation falls to the _ arm rather than pretending.
    let p: baml.ai.Provider = baml.ai.OpenAi { model: "gpt-4o", api_key: "unused", base_url: null };
    let outcome = match (p) {
        let c: baml.ai.Chain => "unexpectedly a chain",
        _ => "not a chain provider",
    };
    assert.equal(outcome, "not a chain provider")
}


// ── LIVE TIER (excluded by default; run with keys via `-i "integ-test-20-*::"`) ────
// The real server-stored chain against OpenAI /v1/responses. `start_chain` teaches a
// fact and stores the response; `extend` re-sends ONLY the follow-up + the handle, yet
// the model recalls the fact — end-to-end proof the server held the context.
testset "integ-test-20-server-chains" {
    test "20 live: extend recalls a fact across the server-stored chain" {
        let p = baml.ai.OpenAiResponses {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let handle = p.start_chain("My favorite number is 41. Remember it. Reply with just: ok") catch (e) {
            _ => baml.ai.ChainHandle { id: "", owner: "openai-responses" },
        };
        log.info("20 live handle id: " + handle.id + " owner: " + handle.owner);

        let ans: string = p.extend<string>(
            "What is my favorite number? Reply with just the number.",
            handle,
        ) catch (e) {
            let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
        };
        log.info("20 live recall answer: " + ans);
        assert.is_true(ans.includes("41"))
    }

    test "20 live: two extends fork from one handle, both recalling the shared root" {
        let p = baml.ai.OpenAiResponses {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        // Root turn: teach one fact and store it server-side.
        let root = p.start_chain("Remember: the magic word is ORANGE. Reply with just: ok") catch (e) {
            _ => baml.ai.ChainHandle { id: "", owner: "openai-responses" },
        };
        log.info("20 live fork root id: " + root.id);

        // Two independent extends from the SAME handle — sibling branches of one root.
        let a: string = p.extend<string>("What is the magic word? Reply with just the word.", root) catch (e) {
            let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
        };
        let b: string = p.extend<string>("State the magic word in lowercase. Reply with just the word.", root) catch (e) {
            let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
        };
        log.info("20 live fork A: " + a + " | fork B: " + b);
        // both branches recalled the shared root fact.
        assert.is_true(a.includes("ORANGE"))
        assert.is_true(b.includes("orange") || b.includes("ORANGE"))
    }
}
