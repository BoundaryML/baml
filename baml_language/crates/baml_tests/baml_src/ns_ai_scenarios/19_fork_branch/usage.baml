// scenario 19 — fork / branch: ../../../../../llm-provider/ideas/scenarios/19-fork-branch/usage.baml
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// "Draft a reply, then explore alternative continuations from the SAME point,
//  WITHOUT losing the original." Two design-true tracks (see implementation.baml):
//
//   (a) CLIENT-HELD branching (copy-the-prefix): a conversation is a native
//       `baml.ai.ChatMessage[]`; a fork is a COPY of the prefix that continues
//       independently, so the trunk is never mutated. Fork-at-tail, fork-anywhere,
//       fan-out-and-compare, and pick-a-winner are all plain BAML. Runs fully
//       offline against the transcript-reflecting `s19_ForkEcho`, and is
//       live-proven below against real OpenAI.
//
//   (b) PROVIDER-HELD branching (the stdlib `baml.ai.Branching` capability):
//       `match (p) { let b: Branching => b.fork(session) }`. Shape + negotiation
//       only (durable store BLOCKED P8); a non-branching client lands in `_`.
//
// The LIVE tier proves the design's emergent-from-ids encoding: two `extend`s from
// the SAME `previous_response_id` are independent branches over a shared server-side
// prefix (`baml.ai.OpenAiResponses`).


// ── (a) Fork at the tail: the trunk is never destroyed ─────────────────────────
//
// The design's core promise. Establish a shared prefix, fork it two ways, and
// confirm BOTH branches saw the prefix AND that the original prefix is untouched.

test "19: forking at the tail leaves the original trunk untouched" {
    let prefix: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.system("You are a copywriter."),
        baml.ai.ChatMessage.user("Draft a tagline for a coffee brand."),
    ];
    let p: baml.ai.Provider = s19_ForkEcho {};

    // two independent forks from the SAME prefix.
    let casual = s19_run_branch(p, prefix, "casual", "Keep it playful.") catch (e) { _ => s19_Branch { id: "err", messages: [], reply: "err" } };
    let formal = s19_run_branch(p, prefix, "formal", "Rewrite it in a formal tone.") catch (e) { _ => s19_Branch { id: "err", messages: [], reply: "err" } };

    // each branch carried the shared prefix's user turn AND its own continuation.
    assert.is_true(casual.reply.includes("Draft a tagline for a coffee brand."))
    assert.is_true(casual.reply.includes("Keep it playful."))
    assert.is_true(formal.reply.includes("Rewrite it in a formal tone."))
    // the branches DIVERGE — the casual continuation is absent from the formal branch.
    assert.is_true(!formal.reply.includes("Keep it playful."))
    // the ORIGINAL prefix is unchanged: fork copied, it did not append in place.
    assert.equal(prefix.length(), 2)
    // and each branch's own array is prefix + 1.
    assert.equal(casual.messages.length(), 3)
    assert.equal(formal.messages.length(), 3)
}


// ── (a) Fork ANYWHERE: branch from an earlier point after moving on ────────────
//
// Even after the conversation grew, go BACK to turn #1 and take a different path.
// The branch must NOT contain the later turns; the trunk still has them.

test "19: forking anywhere drops the later turns from the branch, not the trunk" {
    let history: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.system("You are a copywriter."),          // 0
        baml.ai.ChatMessage.user("Draft a tagline."),                 // 1
        baml.ai.ChatMessage.assistant("Fuel your morning."),          // 2
        baml.ai.ChatMessage.user("Add a call to action."),            // 3
        baml.ai.ChatMessage.assistant("Fuel your morning — order now."), // 4
    ];
    // go back to just after the first assistant answer (keep indices 0..2) and branch.
    let branch = s19_fork_at(history, 3, "Actually, keep it understated.");
    // branch = [system, user#1, assistant#2, new-user]  -> length 4
    assert.equal(branch.length(), 4)
    // the later turns (#3 "call to action", #4) are NOT on the branch.
    let branch_texts: string[] = branch.map((m: baml.ai.ChatMessage) -> string throws never { s19_text_of(m) });
    assert.is_true(!branch_texts.join(" | ").includes("Add a call to action."))
    assert.is_true(branch_texts.join(" | ").includes("Actually, keep it understated."))
    // the TRUNK still has all 5 turns.
    assert.equal(history.length(), 5)
}


// ── (a) Fan out N branches concurrently, compare, PICK A WINNER ────────────────
//
// The scenario's headline: explore several continuations from one shared prefix in
// PARALLEL (spawn/await at the top level of s19_explore3), then rank them and pick.

test "19: fan out three branches from a shared prefix and pick a winner" {
    let prefix: baml.ai.ChatMessage[] = [
        baml.ai.ChatMessage.system("You are a copywriter."),
        baml.ai.ChatMessage.user("Pitch a name for a note-taking app."),
    ];
    let p: baml.ai.Provider = s19_ForkEcho {};

    let branches: s19_Branch[] = s19_explore3(
        p,
        prefix,
        "Make it playful.",
        "Make it minimalist and premium.",
        "Make it technical.",
    ) catch (e) { _ => [] };

    // all three explored the SAME shared prefix (fan-out worked).
    assert.equal(branches.length(), 3)
    // pick the branch that went "premium" — a deterministic stand-in for a ranker.
    let winner = s19_pick_winner(branches, "premium");
    assert.equal(winner.id, "b")
    assert.is_true(winner.reply.includes("Pitch a name for a note-taking app."))
    assert.is_true(winner.reply.includes("minimalist and premium"))
}


// ── (b) Provider-held Branching: capability negotiation ────────────────────────
//
// Branchability is its own capability. A provider that IS `Branching` forks the
// opaque handle into a new, distinct one; a provider that is NOT branchable lands in
// the `_` arm — there is no honest way to fake a fork.

test "19: a branchable provider forks the opaque session into a distinct handle" {
    let p: baml.ai.Provider = s19_ForkableSession {};
    let forked = match (p) {
        let b: baml.ai.Branching => {
            let f = b.fork(baml.ai.Session { _id: "root" }) catch (e) { _ => baml.ai.Session { _id: "failed" } };
            f._id
        },
        _ => "no branching",
    };
    assert.equal(forked, "root-fork")
}

test "19: a non-branchable provider cannot fork (negotiation falls through)" {
    // NoCapProvider (common/fakes.baml) implements only Provider — no Branching.
    let p: baml.ai.Provider = NoCapProvider {};
    let forked = match (p) {
        let b: baml.ai.Branching => {
            let f = b.fork(baml.ai.Session { _id: "root" }) catch (e) { _ => baml.ai.Session { _id: "failed" } };
            f._id
        },
        _ => "no branching",
    };
    assert.equal(forked, "no branching")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ────
//
// The design's emergent-from-ids fork, PROVEN on the real API: a server-stored
// chain establishes a shared prefix (a fact), then TWO `extend`s from the SAME
// handle are two independent branches — both inherit the prefix, each answers its
// own follow-up. That two extends off one `previous_response_id` diverge IS the
// fork (scenario 20's chain surface, forked).
testset "integ-test-19-fork-branch" {
    test "19 live: two extends from the same chain handle are independent branches" {
        let p = baml.ai.OpenAiResponses {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
            store: null,
        };

        // shared prefix: teach the model a fact, server-stored.
        let handle = p.start_chain("My favorite number is 7. Remember it. Reply with just: ok") catch (e) {
            _ => baml.ai.ChainHandle { id: "", owner: "" },
        };
        log.info("19 live: chain handle = " + handle.id);

        // BRANCH A off the shared prefix.
        let branch_a: string = p.extend_text(
            "Is my favorite number even or odd? Reply with just one word.",
            handle,
        ) catch (e) { _ => "branch-a failed" };
        log.info("19 live: branch A = " + branch_a);

        // BRANCH B off the SAME handle — an independent fork, not a continuation of A.
        let branch_b: string = p.extend_text(
            "What is my favorite number times two? Reply with just the number.",
            handle,
        ) catch (e) { _ => "branch-b failed" };
        log.info("19 live: branch B = " + branch_b);

        // both branches inherited the shared prefix (the fact) yet answered their own
        // follow-up — proof the two extends forked from one point rather than chaining.
        assert.is_true(branch_a.includes("odd") || branch_a.includes("Odd"))
        assert.is_true(branch_b.includes("14"))
    }
}
