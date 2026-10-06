// Run from anywhere:
// baml run --file /Users/vbv/repos/baml-3/tools/shep/experiments/codeturtle-policy.baml --output-format json
// A deterministic extension experiment; no model calls or GitHub mutations.

enum PolicyMatch {
    Applies,
    DoesNotApply,
    Uncertain,
}

class FileChange {
    path: string,
    added_code: string,
}

class PolicyResult {
    outcome: PolicyMatch,
    reason: string,
    evidence: string[],
}

class ReviewRequirement {
    path: string,
    assessment: PolicyResult,
    human_review_required: bool,
    owners: string[],
    require: string,
}

type ReviewPolicy = (FileChange) -> PolicyResult throws never;

function main() -> ReviewRequirement[] {
    // This source stands in for an approved repo's .codeturtle/policy.baml.
    // Text matching is deliberately a small demo, not a complete env detector.
    let extension = reflect.Package.compile(
        {
            "policy.baml": `
                function AddsEnvironmentVariable(change: codeturtle.FileChange) -> codeturtle.PolicyResult throws never {
                    if (change.added_code.includes("std::env::var(")) {
                        codeturtle.PolicyResult {
                            outcome: codeturtle.PolicyMatch.Applies,
                            reason: "Adds a direct environment read; both environment owners must review.",
                            evidence: [change.path + ": " + change.added_code],
                        }
                    } else if (change.added_code.includes("BAML_")) {
                        codeturtle.PolicyResult {
                            outcome: codeturtle.PolicyMatch.Uncertain,
                            reason: "Mentions a BAML variable, but this detector cannot establish whether it adds an environment read.",
                            evidence: [change.path + ": " + change.added_code],
                        }
                    } else {
                        codeturtle.PolicyResult {
                            outcome: codeturtle.PolicyMatch.DoesNotApply,
                            reason: "No environment signal matched this demo rule.",
                            evidence: [],
                        }
                    }
                }
            `,
        },
        packages = { "codeturtle": reflect.Package.current() },
    );
    let assess = extension.get_function<ReviewPolicy>("root.AddsEnvironmentVariable")
        ?? throw "Missing policy function";

    let changes = [
        FileChange {
            path: "src/timeout.rs",
            added_code: `let timeout = std::env::var("BAML_REQUEST_TIMEOUT");`,
        },
        FileChange { path: "src/greeting.rs", added_code: `let greeting = "hello";` },
        FileChange {
            path: "src/config.rs",
            added_code: `let timeout = config.lookup("BAML_REQUEST_TIMEOUT");`,
        },
    ];
    let requirements: ReviewRequirement[] = [];
    for (let change in changes) {
        let assessment = assess(change);
        let needs_review = assessment.outcome != PolicyMatch.DoesNotApply;
        requirements.push(
            ReviewRequirement {
                path: change.path,
                assessment: assessment,
                human_review_required: needs_review,
                owners: if (needs_review) {
                    ["@hellovai", "@aaronvg"]
                } else {
                    []
                },
                require: if (needs_review) {
                    "all"
                } else {
                    "none_for_this_rule"
                },
            },
        );
    }
    assert.equal(requirements[0].assessment.outcome, PolicyMatch.Applies);
    assert.equal(requirements[0].human_review_required, true);
    assert.equal(requirements[1].assessment.outcome, PolicyMatch.DoesNotApply);
    assert.equal(requirements[1].human_review_required, false);
    assert.equal(requirements[2].assessment.outcome, PolicyMatch.Uncertain);
    assert.equal(requirements[2].human_review_required, true);
    requirements
}
