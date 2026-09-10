// Conservative title matching within the current dataset and compiler version.
function title_terms(title: string) -> string[] {
    let normalized = title.to_lower_case()
        .chars()
        .map((c: string) -> string {
            if (c.is_alphanumeric() || c == "_") {
                c
            } else {
                " "
            }
        })
        .join("");
    let skip = "a an the is are be being been was were to of on in at by for from with and or it its this that may should could would can not just every instead verify claim unconfirmed function baml issue"
        .split(" ");
    let terms: string[] = [];
    for (let raw in normalized.split(" ")) {
        let word = match (raw) {
            "compiler" | "compilation" | "compiles" | "compiling" => "compile",
            "returns" | "returning" => "return",
            "rejected" | "rejects" | "rejecting" => "reject",
            "accepted" | "accepts" | "accepting" => "accept",
            _ => raw,
        };
        if (word.length() > 0 && !skip.includes(word) && !terms.includes(word)) {
            terms.push(word);
        };
    }
    terms
}

function similar_issue_titles(a: string, b: string) -> bool {
    let left = title_terms(a);
    let right = title_terms(b);
    if (left.length() == 0 || right.length() == 0) {
        return false;
    };
    let overlap = left
        .filter((w: string) -> bool {
            right.includes(w)
        })
        .length();
    // Exact normalized titles, or Dice similarity >= 0.60 with four shared terms.
    (overlap == left.length() && overlap == right.length())
        || (overlap >= 4 && overlap * 200 >= (left.length() + right.length()) * 60)
}

function duplicate_issue(candidate: Issue) -> Issue? {
    let matches: Issue[] = [];
    for (let state in ["open", "awaiting_approval", "approved", "in_progress"]) {
        for (let existing in load_issues_in(state)) {
            if (
                existing.id != candidate.id
                    && existing.subsystem == candidate.subsystem
                    && existing.version == candidate.version
                    && similar_issue_titles(existing.title, candidate.title)
            ) {
                matches.push(existing);
            };
        }
    }
    // Stable tie-break when older data already has more than one matching issue.
    let chosen: Issue? = null;
    for (let issue in matches) {
        if (chosen == null || issue.id < (chosen ?? issue).id) {
            chosen = issue;
        };
    }
    chosen
}

function merge_duplicate(existing: Issue, candidate: Issue) -> Issue {
    let repros = existing.repros;
    let ids = existing.feedback_ids;
    for (let repro in candidate.repros) {
        if (!repros.includes(repro)) {
            repros.push(repro);
        };
    }
    for (let id in candidate.feedback_ids) {
        if (!ids.includes(id)) {
            ids.push(id);
        };
    }
    let row = baml.json.from_json<map<string, json>>(issue_row(existing));
    row.set("repros", repros.to_json());
    row.set("feedback_ids", ids.to_json());
    issue_from_row(row.to_json())
}

testset "title_dedup" {
    test "matches wording changes but not unrelated bugs" {
        assert.is_true(
            similar_issue_titles(
                "Function return type _ may be accepted by compiler instead of being rejected",
                "Compiler rejects _ function return type",
            ),
        );
        assert.is_true(
            similar_issue_titles("Parser panics on nested unions", "parser: panics on nested unions"),
        );
        assert.is_true(
            !similar_issue_titles("Parser panics on nested unions", "Parser panics on missing semicolons"),
        );
        assert.is_true(!similar_issue_titles("", ""));
    }
    test "merging retains lifecycle and adds each report and repro once" {
        let issue = gh_issue(7, Difficulty.Medium);
        let merged = merge_duplicate(issue, issue);
        assert.equal(merged.id, issue.id);
        assert.equal(merged.status, issue.status);
        assert.equal(merged.repros, issue.repros);
        assert.equal(merged.feedback_ids, issue.feedback_ids);
    }
}
