/// Return only public API files that lack an accompanying API doc change.
function undocumented_public_api_change(changes: codeturtle.FileChange[]) -> codeturtle.Match[] throws never {
    let docs_changed = false;
    for (let change in changes) {
        if (change.path == "docs/api.md") {
            docs_changed = true;
        }
    }
    let results: codeturtle.Match[] = [];
    if (!docs_changed) {
        for (let change in changes) {
            if (change.path.starts_with("src/api/") && change.added_code.includes("pub fn ")) {
                results.push(
                    codeturtle.Match {
                        path: change.path,
                        verdict: codeturtle.Verdict.Applies,
                        reason: "Changes the public API without changing docs/api.md.",
                        evidence: [change.added_code],
                    },
                );
            }
        }
    }
    results
}
