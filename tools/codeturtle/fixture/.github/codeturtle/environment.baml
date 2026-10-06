/// A deterministic text detector for this experiment, not a complete env parser.
function adds_environment_variable(change: codeturtle.FileChange) -> codeturtle.Match throws never {
    if (change.added_code.includes("std::env::var(")) {
        codeturtle.Match {
            path: change.path,
            verdict: codeturtle.Verdict.Applies,
            reason: "Introduces a direct environment read.",
            evidence: [change.added_code],
        }
    } else if (change.added_code.includes("BAML_")) {
        codeturtle.Match {
            path: change.path,
            verdict: codeturtle.Verdict.Uncertain,
            reason: "Mentions a BAML variable; inspect whether it introduces an environment read.",
            evidence: [change.added_code],
        }
    } else {
        codeturtle.Match {
            path: change.path,
            verdict: codeturtle.Verdict.DoesNotApply,
            reason: "No environment signal matched.",
            evidence: [],
        }
    }
}
