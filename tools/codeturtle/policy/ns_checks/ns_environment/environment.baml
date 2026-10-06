/// Conservatively detect added Rust environment reads with lexical matching.
/// This is a review trigger, not proof that a new variable was introduced.
function adds_environment_read(change: codeturtle.FileChange) -> codeturtle.Match throws never {
    let reads: string[] = [];
    if (change.path.ends_with(".rs")) {
        for (let line in change.added_code.lines()) {
            if (
                line.includes("env::var(")
                    || line.includes("env::var_os(")
                    || line.includes("env::vars(")
                    || line.includes("env::vars_os(")
            ) {
                reads.push(line);
            }
        }
    }
    if (reads.length() > 0) {
        codeturtle.Match {
            path: change.path,
            verdict: codeturtle.Verdict.Applies,
            reason: "Adds a direct Rust environment read. verify that runtime configuration uses the approved baml_env entry point.",
            evidence: reads,
        }
    } else {
        codeturtle.Match {
            path: change.path,
            verdict: codeturtle.Verdict.DoesNotApply,
            reason: "No added direct Rust environment read matched this rule.",
            evidence: [],
        }
    }
}
