// issue -> difficulty. An LLM reads the ticket (description, repros,
// resolution plan) and estimates how much work it is; gauge_issue records
// the estimate on the issue. Idempotent: an issue that already has a
// difficulty keeps it.

/// The estimate and why. `rationale` is what a reviewer checks when the
/// estimate looks off.
class DifficultyEstimate {
    difficulty: Difficulty,
    /// One or two sentences: what makes it this hard and not the next level
    /// up or down.
    rationale: string,
    /// What is still unknown that could move the estimate (empty when the
    /// cause is clear).
    unknowns: string[],
}

// Difficulty is a judgment call the rest of the pipeline is not; it runs
// on Opus, the other stages on Triage (Sonnet).
client Gauge = claude_code.ClaudeCodeClient.new(model = "opus", timeout_ms = 900000);

function estimate_difficulty(issue: Issue) -> DifficultyEstimate {
    client: Gauge
    prompt: `
        Estimate how much engineering work this BAML toolchain issue is, from
        the ticket alone. Judge the FIX, not the symptom: a crash can be a
        one-line guard, a cosmetic mismatch can need a redesign. For a
        feature request, judge the IMPLEMENTATION of the proposed feature as
        written, including its tests and docs.

        - trivial: one obvious change — a typo, a message, a default, a
          missing match arm.
        - easy: a localized fix in one place with a clear cause; under a
          day. The resolution plan already names the spot.
        - medium: a few days inside one subsystem; the cause is known or
          nearly so, but the fix touches several places or needs new tests
          or a small design decision.
        - hard: design work, cross-subsystem changes, a root cause that is
          still unknown, or behavior that must change for many users.

        Weigh: whether the root cause is identified; how many surfaces the
        repros span; whether the resolution plan is concrete; whether a
        fix changes semantics others depend on.

        Issue ID: ${issue.id}
        Kind: ${issue_kind(issue).to_string()}
        Title: ${issue.title}
        Subsystem: ${issue.subsystem.to_string()}

        Description:
        ${issue.description}

        ${if (issue_kind(issue) == IssueKind.Feature) { "Proposed feature:" } else { "Resolution plan:" }}
        ${issue.resolution_plan ?? "(none)"}

        Repros (${issue.repros.length().to_string()}):
        ${format_repros(issue.repros)}

        ${ctx.output_format()}
    `
}

function format_repros(repros: Repro[]) -> string {
    repros
        .map((r: Repro) -> string {
            "$ " + r.command + " — files: " + r.files.keys().join(", ")
        })
        .join("\n")
}

/// Record the difficulty on an issue. Idempotent: an issue that already
/// has one keeps it.
function gauge_issue(issue: Issue) -> Issue {
    //# keep an existing difficulty
    if (issue.difficulty != null) {
        return issue;
    };
    //# estimate from the ticket (LLM, Opus)
    let estimate = estimate_difficulty(issue);
    log.info(
        {
            "issue": issue.id,
            "difficulty": estimate.difficulty.to_string(),
            "rationale": estimate.rationale,
            "unknowns": estimate.unknowns,
        },
    );
    log_event(
        "gauged",
        issue.id,
        null,
        {
            "step": "Gauge difficulty",
            "decision": estimate.difficulty.to_string()
                + ": "
                + match (estimate.difficulty) {
                    Difficulty.Trivial => "one obvious change",
                    Difficulty.Easy => "a localized fix, under a day",
                    Difficulty.Medium => "a few days inside one subsystem",
                    Difficulty.Hard => "design or cross-subsystem work, or the cause is unknown",
                },
            "reason": estimate.rationale,
            "evidence": { "unknowns": estimate.unknowns.to_json(), "repros": issue.repros.length().to_json() }.to_json(),
            "summary": estimate.rationale,
            "difficulty": estimate.difficulty.to_string(),
        }
            .to_json(),
        null,
        null,
    );
    //# record it on the issue
    with_difficulty(issue, estimate.difficulty)
}

function with_difficulty(issue: Issue, difficulty: Difficulty) -> Issue {
    Issue {
        id: issue.id,
        kind: issue.kind,
        title: issue.title,
        description: issue.description,
        shepherd: issue.shepherd,
        subsystem: issue.subsystem,
        repros: issue.repros,
        version: issue.version,
        feedback_ids: issue.feedback_ids,
        status: issue.status,
        comments: issue.comments,
        resolution_plan: issue.resolution_plan,
        difficulty: difficulty,
        design_doc: issue.design_doc,
        slack_ts: issue.slack_ts,
        slack_channel: issue.slack_channel,
    }
}
