/// Detect behavior changes in PRs whose title claims they only change docs.
function docs_only_behavior_change(pr: codeturtle.PullRequest) -> codeturtle.Match[] throws never {
    let results: codeturtle.Match[] = [];
    if (pr.title.starts_with("Docs:")) {
        for (let change in pr.changes) {
            if (change.path.starts_with("src/")) {
                results.push(
                    codeturtle.Match {
                        path: change.path,
                        verdict: codeturtle.Verdict.Applies,
                        reason: "The PR title claims docs-only, but source code also changes.",
                        evidence: [pr.title, change.path],
                    },
                );
            }
        }
    }
    results
}
