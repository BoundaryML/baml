function SummarizeAttemptFindings(
    claude_executable: string,
    workspace: string,
    failures_json: string,
    describes_json: string,
    final_verification_passed: bool,
) -> FindingDrafts {
    client: claude_code.ClaudeCodeClient.new(
        executable = claude_executable,
        model = "sonnet",
        cwd = workspace,
        timeout_ms = 360000,
        permission_mode = "plan",
        harness_tools = [],
    )
    prompt: `
        Identify only the distinct issues this coding agent encountered.

        The report is for maintainers deciding whether to change a BAML coding
        skill. Keep every title understandable without knowing the generated
        implementation. Do not mention internal names such as local helper types
        unless you explain what they mean in plain English.

        Classify an issue as SkillGap only when the evidence plausibly indicates
        missing, incorrect, or unclear BAML guidance. For SkillGap, provide one
        short concrete documentation change. Classify ordinary coding mistakes
        as ImplementationIssue and leave suggested_skill_change null.

        Group repetitions. Ignore harmless exploratory failures outside the
        benchmark workspace. A later successful command or subcommand generally
        means the issue was resolved. An invocation still marked running is
        unfinished, not proof that a process survived. Final verification is
        authoritative for the final project state.

        Return at most five findings. Each title and suggested change must be one
        short sentence. Cite only exact invocation IDs from the input.

        Failed or unfinished BAML invocations:
        ${failures_json}

        All baml describe invocations:
        ${describes_json}

        Final verification passed: ${final_verification_passed}

        ${ctx.output_format()}
    `
}

function concise(value: string) -> string {
    truncate_text(value.trim(), 240)
}

function validate_findings(
    attempt_id: string,
    failures: FailedInvocation[],
    drafts: FindingDrafts,
) -> ValidatedFindings {
    let known_ids = failures.map((failure) -> {
        failure.invocation_id
    });
    let findings: Finding[] = [];
    let invalid_ids: string[] = [];

    for (let draft in drafts.findings.slice(0, 5)) {
        let evidence: EvidenceRef[] = [];
        for (let id in draft.invocation_ids) {
            if (known_ids.includes(id)) {
                if (
                    !evidence.some((reference) -> {
                        reference.invocation_id == id
                    })
                ) {
                    evidence.push(EvidenceRef { attempt_id: attempt_id, invocation_id: id });
                }
            } else if (!invalid_ids.includes(id)) {
                invalid_ids.push(id);
            }
        }
        if (evidence.length() > 0) {
            findings.push(
                Finding {
                    kind: draft.kind,
                    category: draft.category,
                    title: concise(draft.title),
                    suggested_skill_change: if let change: string = draft.suggested_skill_change {
                        concise(change)
                    } else {
                        null
                    },
                    evidence: evidence,
                    resolved: draft.resolved,
                },
            );
        }
    }
    ValidatedFindings { findings: findings, invalid_evidence_ids: invalid_ids }
}

function summarize_attempt(
    attempt_id: string,
    claude_executable: string,
    workspace: string,
    failures: FailedInvocation[],
    describes: DescribeCall[],
    final_verification_passed: bool,
) -> ValidatedFindings {
    if (failures.length() == 0) {
        return ValidatedFindings { findings: [], invalid_evidence_ids: [] };
    }
    let drafts = SummarizeAttemptFindings(
        claude_executable,
        workspace,
        baml.json.stringify_pretty(baml.json.to_json(failures)),
        baml.json.stringify_pretty(baml.json.to_json(describes)),
        final_verification_passed,
    );
    validate_findings(attempt_id, failures, drafts)
}
