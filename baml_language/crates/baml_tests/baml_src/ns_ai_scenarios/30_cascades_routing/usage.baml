// scenario 30 — cascades & routing: ../../../../../llm-provider/ideas/scenarios/30-cascades-routing/README.md
// Escalate to the expensive provider when the cheap one is unsure — the
// presence-not-calibration caveat (plan D3.iv) is inherent.

function s30_cascade(
    cheap: baml.ai.Provider,
    expensive: baml.ai.Provider,
    q: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    let first = match (cheap) {
        let h: baml.ai.HttpProvider => h.call<string>(q),
        _ => throw baml.errors.Unsupported { message: "cheap stage not callable" },
    };
    if (!first.includes("UNSURE")) {
        return first;
    }
    match (expensive) {
        let h: baml.ai.HttpProvider => h.call<string>(q),
        _ => throw baml.errors.Unsupported { message: "expensive stage not callable" },
    }
}

test "30: a confident cheap answer never escalates" {
    let r = s30_cascade(EchoProvider { reply: "42" }, EchoProvider { reply: "expensive 42" }, "q") catch (e) { _ => "failed" };
    assert.equal(r, "42")
}

test "30: an unsure cheap answer escalates to the expensive provider" {
    let r = s30_cascade(EchoProvider { reply: "UNSURE" }, EchoProvider { reply: "expensive verdict" }, "q") catch (e) { _ => "failed" };
    assert.equal(r, "expensive verdict")
}
