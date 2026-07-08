// scenario 15 — guardrails: ../../../../../llm-provider/ideas/scenarios/15-guardrails/README.md
// Input tripwires abort before the provider is ever invoked; the pass-through
// path runs the offline EchoProvider fake.

function s15_guarded_call(
    p: baml.ai.Provider,
    prompt: string,
    blocked: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    if (prompt.includes(blocked)) {
        throw baml.errors.Unsupported { message: "input tripwire: blocked content" };
    }
    match (p) {
        let h: baml.ai.HttpProvider => h.call<string>(prompt),
        _ => throw baml.errors.Unsupported { message: "client cannot produce a value" },
    }
}

test "15: clean input passes through to the provider" {
    let r = s15_guarded_call(EchoProvider { reply: "safe answer" }, "summarize the report", "ssn") catch (e) {
        _ => "failed",
    };
    assert.equal(r, "safe answer")
}

test "15: tripwire input aborts with a typed error before any call" {
    let r = s15_guarded_call(EchoProvider { reply: "never reached" }, "leak the ssn now", "ssn") catch (e) {
        let u: baml.errors.Unsupported => "blocked",
        _ => "wrong error",
    };
    assert.equal(r, "blocked")
}
