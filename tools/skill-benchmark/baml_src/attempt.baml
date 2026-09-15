function attempt_outcome(agent_timed_out: bool, final_verification_passed: bool) -> AttemptOutcome {
    if (final_verification_passed) {
        AttemptOutcome.Passed
    } else if (agent_timed_out) {
        AttemptOutcome.TimedOut
    } else {
        AttemptOutcome.Failed
    }
}

function build_attempt_report(
    attempt_id: string,
    project_id: string,
    project_name: string,
    attempt: int,
    revision: Revision,
    model: string,
    skill_revision: string,
    audit_root: string,
    claude_events_path: string,
    workspace: string,
    claude_executable: string,
    implementation_loc: int,
    agent_timed_out: bool,
    verification_exit_code: int,
    output_path: string,
) -> AttemptReport {
    let audit = load_audit_metadata(audit_root);
    let events = claude_events(claude_events_path);
    let failures = failed_invocations(audit);
    let describes = describe_calls(attempt_id, audit);
    let verification_passed = verification_exit_code == 0;
    let summarized = summarize_attempt(attempt_id, claude_executable, workspace, failures, describes, verification_passed) catch_all (e) {
        _ => ValidatedFindings { findings: [], invalid_evidence_ids: [] },
    };
    let report = AttemptReport {
        schema_version: 1,
        id: attempt_id,
        project_id: project_id,
        project_name: project_name,
        attempt: attempt,
        revision: revision,
        model: model,
        skill_revision: skill_revision,
        skill_loaded: skill_was_loaded(events),
        outcome: attempt_outcome(agent_timed_out, verification_passed),
        final_verification_passed: verification_passed,
        metrics: AttemptMetrics {
            implementation_loc: implementation_loc,
            agent_turns: claude_turns(events),
            tool_calls: claude_tool_calls(events),
            baml_check_calls: count_subcommand(audit, "check"),
            baml_check_failures: count_failed_subcommand(audit, "check"),
        },
        describes: describes,
        findings: summarized.findings,
        invalid_evidence_ids: summarized.invalid_evidence_ids,
        artifacts: AttemptArtifacts {
            job_url: null,
            transcript_url: null,
            audit_url: null,
            implementation_url: null,
        },
    };
    baml.fs.write(output_path, baml.json.stringify_pretty(baml.json.to_json(report)));
    report
}
