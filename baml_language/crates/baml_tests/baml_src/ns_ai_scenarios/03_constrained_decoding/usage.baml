// scenario 03 — constrained decoding: ../../../../../llm-provider/ideas/scenarios/03-constrained-decoding/README.md
// Constraint support is a runtime promise over the existential (the honest
// B1 story): negotiate, and the absence is typed — never silent.

function s03_require_constrained(p: baml.ai.Provider) -> string throws baml.errors.Unsupported {
    match (p) {
        let c: baml.ai.Constrained => "constrained-capable",
        _ => throw baml.errors.Unsupported { message: "provider cannot hard-constrain decoding" },
    }
}

test "03: a provider without Constrained is a typed refusal, not silent degradation" {
    let r = s03_require_constrained(EchoProvider { reply: "x" }) catch (e) {
        let u: baml.errors.Unsupported => "honest refusal",
        _ => "wrong error",
    };
    assert.equal(r, "honest refusal")
}
