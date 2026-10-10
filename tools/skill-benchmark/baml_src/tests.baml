function test_metrics(
    attempts: int,
    completed: int,
    turns: float,
    tools: float,
    failures: float,
) -> AggregateMetrics {
    AggregateMetrics {
        attempts: attempts,
        completed_attempts: completed,
        implementation_loc_avg: 100.0,
        agent_turns_avg: turns,
        tool_calls_avg: tools,
        baml_check_failures_avg: failures,
    }
}

function test_attempt(id: string, outcome: AttemptOutcome) -> AttemptReport {
    AttemptReport {
        schema_version: 1,
        id: id,
        project_id: "chess",
        project_name: "Chess engine",
        attempt: 1,
        revision: Revision.Head,
        model: "opus",
        skill_revision: "head",
        skill_loaded: true,
        outcome: outcome,
        final_verification_passed: outcome == AttemptOutcome.Passed,
        metrics: AttemptMetrics {
            implementation_loc: 100,
            agent_turns: 10,
            tool_calls: 20,
            baml_check_calls: 3,
            baml_check_failures: 1,
        },
        describes: [],
        findings: [],
        invalid_evidence_ids: [],
        artifacts: AttemptArtifacts {
            job_url: null,
            transcript_url: null,
            audit_url: null,
            implementation_url: null,
        },
    }
}

function test_manifest(reports: AttemptReport[]) -> BenchmarkManifest {
    BenchmarkManifest {
        schema_version: 1,
        generated_at: "2026-09-02T00:00:00Z",
        skill_revision: "head",
        suite_revision: "suite",
        model: "opus",
        attempts_per_project: 1,
        reports: reports,
    }
}

test "classification requires a baseline" {
    assert.equal(
        classify_change(null, test_metrics(3, 3, 20.0, 30.0, 1.0)),
        ChangeClassification.BaselineUnavailable,
    )
}

test "a completion drop is a likely regression" {
    assert.equal(
        classify_change(test_metrics(3, 3, 20.0, 30.0, 1.0), test_metrics(3, 2, 10.0, 15.0, 0.0)),
        ChangeClassification.LikelyRegression,
    )
}

test "two meaningful efficiency gains are a likely improvement" {
    assert.equal(
        classify_change(test_metrics(3, 3, 20.0, 30.0, 2.0), test_metrics(3, 3, 15.0, 24.0, 2.0)),
        ChangeClassification.LikelyImprovement,
    )
}

test "small changes are likely neutral" {
    assert.equal(
        classify_change(test_metrics(3, 3, 20.0, 30.0, 2.0), test_metrics(3, 3, 19.0, 29.0, 2.0)),
        ChangeClassification.LikelyNeutral,
    )
}

test "one project regression makes the overall result a regression" {
    let attempt = test_attempt("chess-head-1", AttemptOutcome.Failed);
    assert.equal(
        classify_projects(
            [
                ProjectReport {
                    id: "chess",
                    name: "Chess engine",
                    classification: ChangeClassification.LikelyRegression,
                    base: test_metrics(3, 3, 20.0, 30.0, 1.0),
                    head: test_metrics(3, 2, 20.0, 30.0, 1.0),
                    base_attempts: [],
                    head_attempts: [attempt],
                },
            ],
        ),
        ChangeClassification.LikelyRegression,
    )
}

test "project reports use the matching attempt name" {
    let attempt = test_attempt("chess-head-1", AttemptOutcome.Passed);
    let projects = build_project_reports(test_manifest([attempt]), null);
    assert.equal(projects.length(), 1);
    assert.equal(projects.at(0)?.id, "chess");
    assert.equal(projects.at(0)?.name, "Chess engine")
}

test "project reports include matching baseline attempts" {
    let head_attempt = test_attempt("chess-head-1", AttemptOutcome.Passed);
    let base_attempt = test_attempt("chess-base-1", AttemptOutcome.Passed);
    let projects = build_project_reports(test_manifest([head_attempt]), test_manifest([base_attempt]));
    let project = projects.at(0) ?? baml.sys.panic("missing project report");
    assert.equal(project.base_attempts.length(), 1);
    assert.equal(project.classification, ChangeClassification.LikelyNeutral)
}

test "renderer creates one project detail block" {
    let attempt = test_attempt("chess-head-1", AttemptOutcome.Passed);
    let project = ProjectReport {
        id: "chess",
        name: "Chess engine",
        classification: ChangeClassification.BaselineUnavailable,
        base: null,
        head: aggregate_attempts([attempt]),
        base_attempts: [],
        head_attempts: [attempt],
    };
    let markdown = render_benchmark_report(
        BenchmarkReport {
            schema_version: 1,
            classification: ChangeClassification.BaselineUnavailable,
            baseline_status: BaselineStatus.Missing,
            baseline_differences: [],
            base_skill_revision: null,
            head_skill_revision: "head",
            suite_revision: "suite",
            model: "opus",
            projects: [project],
        },
        ReportLinks { attempts: [], evidence: [] },
    );
    assert.is_true(markdown.includes("## Overall result: Baseline unavailable"));
    assert.equal(markdown.split("<details>").length(), 2);
    assert.is_true(markdown.includes("### `baml describe` usage"))
}
