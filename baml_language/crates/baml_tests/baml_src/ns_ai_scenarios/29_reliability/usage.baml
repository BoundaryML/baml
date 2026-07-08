// scenario 29 — reliability: ../../../../../llm-provider/ideas/scenarios/29-reliability/README.md
// Combinators over the existential; the effect-aware refusal is in
// 10_agentic_loop ("retry refuses..."). Rust mock tier covers retry counts.

test "29: Fallback routes past a failing member to the first working one" {
    let p: baml.ai.Provider = baml.ai.Fallback { members: [AlwaysFailProvider {}, EchoProvider { reply: "recovered" }] };
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "recovered")
}

test "29: fluent combinator factories chain off any provider" {
    let chained: baml.ai.Provider = AlwaysFailProvider {}.fallback_to(EchoProvider { reply: "via fluent" });
    let r = match (chained) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "all failed" },
        _ => "not http",
    };
    assert.equal(r, "via fluent")
}

test "29: Retry exhausts its budget over a failing provider and throws" {
    let p: baml.ai.Provider = AlwaysFailProvider {}.with_retry(2);
    let r = match (p) {
        let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "exhausted" },
        _ => "not http",
    };
    assert.equal(r, "exhausted")
}
