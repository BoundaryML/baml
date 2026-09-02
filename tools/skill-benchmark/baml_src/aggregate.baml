function load_attempt_reports(directory: string) -> AttemptReport[] {
    let reports: AttemptReport[] = [];
    if (!baml.fs.exists(directory)) {
        return reports;
    }
    for (let entry in baml.fs.read_dir(directory)) {
        let path = `${directory}/${entry.name}`;
        if (!entry.is_dir && entry.name.ends_with(".json")) {
            let report = baml.json.from_string<AttemptReport>(baml.fs.read(path)) catch_all (e) {
                _ => null,
            };
            if let value: AttemptReport = report {
                reports.push(value);
            }
        }
    }
    reports.sort_by_key((report) -> {
        `${report.project_id}:${report.attempt}`
    })
}

function attach_attempt_artifact(report_path: string, artifact_url: string) -> AttemptReport {
    let report = baml.json.from_string<AttemptReport>(baml.fs.read(report_path));
    report.artifacts.transcript_url = artifact_url;
    report.artifacts.audit_url = artifact_url;
    report.artifacts.implementation_url = artifact_url;
    baml.fs.write(report_path, baml.json.stringify_pretty(baml.json.to_json(report)));
    report
}

function build_manifest(
    reports_directory: string,
    generated_at: string,
    skill_revision: string,
    suite_revision: string,
    model: string,
    attempts_per_project: int,
    output_path: string,
) -> BenchmarkManifest {
    let manifest = BenchmarkManifest {
        schema_version: 1,
        generated_at: generated_at,
        skill_revision: skill_revision,
        suite_revision: suite_revision,
        model: model,
        attempts_per_project: attempts_per_project,
        reports: load_attempt_reports(reports_directory),
    };
    baml.fs.write(output_path, baml.json.stringify_pretty(baml.json.to_json(manifest)));
    manifest
}

function average_int(values: int[]) -> float? {
    if (values.length() == 0) {
        null
    } else {
        values.reduce(
            (sum, value) -> {
                sum + value
            },
            0,
        )
            * 1.0
            / values.length()
    }
}

function aggregate_attempts(attempts: AttemptReport[]) -> AggregateMetrics {
    AggregateMetrics {
        attempts: attempts.length(),
        completed_attempts: attempts
            .filter((attempt) -> {
                attempt.outcome == AttemptOutcome.Passed
            })
            .length(),
        implementation_loc_avg: average_int(
            attempts.map((attempt) -> {
                attempt.metrics.implementation_loc
            }),
        ),
        agent_turns_avg: average_int(
            attempts.filter_map((attempt) -> {
                attempt.metrics.agent_turns
            }),
        ),
        tool_calls_avg: average_int(
            attempts.map((attempt) -> {
                attempt.metrics.tool_calls
            }),
        ),
        baml_check_failures_avg: average_int(
            attempts.map((attempt) -> {
                attempt.metrics.baml_check_failures
            }),
        ),
    }
}

function meaningfully_lower(head: float?, base: float?) -> bool {
    if let head_value: float = head {
        if let base_value: float = base {
            base_value > 0.0 && head_value <= base_value * 0.9
        } else {
            false
        }
    } else {
        false
    }
}

function meaningfully_higher(head: float?, base: float?) -> bool {
    if let head_value: float = head {
        if let base_value: float = base {
            if (base_value == 0.0) {
                head_value >= 1.0
            } else {
                head_value >= base_value * 1.1
            }
        } else {
            false
        }
    } else {
        false
    }
}

function classify_change(base: AggregateMetrics?, head: AggregateMetrics) -> ChangeClassification {
    if let baseline: AggregateMetrics = base {
        let base_completion = if (baseline.attempts == 0) {
            0.0
        } else {
            baseline.completed_attempts * 1.0 / baseline.attempts
        };
        let head_completion = if (head.attempts == 0) {
            0.0
        } else {
            head.completed_attempts * 1.0 / head.attempts
        };
        if (head_completion < base_completion) {
            return ChangeClassification.LikelyRegression;
        }

        let improvements = 0;
        let regressions = 0;
        if (meaningfully_lower(head.agent_turns_avg, baseline.agent_turns_avg)) {
            improvements += 1;
        } else if (meaningfully_higher(head.agent_turns_avg, baseline.agent_turns_avg)) {
            regressions += 1;
        }
        if (meaningfully_lower(head.tool_calls_avg, baseline.tool_calls_avg)) {
            improvements += 1;
        } else if (meaningfully_higher(head.tool_calls_avg, baseline.tool_calls_avg)) {
            regressions += 1;
        }
        if (meaningfully_lower(head.baml_check_failures_avg, baseline.baml_check_failures_avg)) {
            improvements += 1;
        } else if (meaningfully_higher(head.baml_check_failures_avg, baseline.baml_check_failures_avg)) {
            regressions += 1;
        }

        if (regressions >= 2) {
            ChangeClassification.LikelyRegression
        } else if (improvements >= 2 && regressions == 0) {
            ChangeClassification.LikelyImprovement
        } else {
            ChangeClassification.LikelyNeutral
        }
    } else {
        ChangeClassification.BaselineUnavailable
    }
}

function select_baseline(head: BenchmarkManifest, path: string?) -> BaselineSelection {
    if let baseline_path: string = path {
        if (baml.fs.exists(baseline_path)) {
            let baseline = baml.json.from_string<BenchmarkManifest>(baml.fs.read(baseline_path));
            let differences: string[] = [];
            if (baseline.schema_version != head.schema_version) {
                differences.push("report schema");
            }
            if (baseline.suite_revision != head.suite_revision) {
                differences.push("benchmark suite or harness");
            }
            if (baseline.model != head.model) {
                differences.push("model");
            }
            if (baseline.attempts_per_project != head.attempts_per_project) {
                differences.push("attempt count");
            }
            if (differences.length() == 0) {
                return BaselineSelection {
                    status: BaselineStatus.Available,
                    differences: [],
                    manifest: baseline,
                };
            }
            return BaselineSelection {
                status: BaselineStatus.Incompatible,
                differences: differences,
                manifest: null,
            };
        }
    }
    BaselineSelection { status: BaselineStatus.Missing, differences: [], manifest: null }
}

function project_ids(reports: AttemptReport[]) -> string[] {
    let ids: string[] = [];
    for (let report in reports) {
        if (!ids.includes(report.project_id)) {
            ids.push(report.project_id);
        }
    }
    ids
}

function build_project_reports(
    head: BenchmarkManifest,
    baseline: BenchmarkManifest?,
) -> ProjectReport[] {
    let projects: ProjectReport[] = [];
    for (let id in project_ids(head.reports)) {
        let head_attempts = head.reports.filter((attempt) -> {
            attempt.project_id == id
        });
        let base_attempts = if let base: BenchmarkManifest = baseline {
            base.reports.filter((attempt) -> {
                attempt.project_id == id
            })
        } else {
            []
        };
        let base_metrics = if (base_attempts.length() > 0) {
            aggregate_attempts(base_attempts)
        } else {
            null
        };
        let head_metrics = aggregate_attempts(head_attempts);
        projects.push(
            ProjectReport {
                id: id,
                name: head_attempts[0].project_name,
                classification: classify_change(base_metrics, head_metrics),
                base: base_metrics,
                head: head_metrics,
                base_attempts: base_attempts,
                head_attempts: head_attempts,
            },
        );
    }
    projects
}

function classify_projects(projects: ProjectReport[]) -> ChangeClassification {
    if (
        projects.some((project) -> {
            project.classification == ChangeClassification.BaselineUnavailable
        })
    ) {
        return ChangeClassification.BaselineUnavailable;
    }
    if (
        projects.some((project) -> {
            project.classification == ChangeClassification.LikelyRegression
        })
    ) {
        return ChangeClassification.LikelyRegression;
    }
    if (
        projects
            .filter((project) -> {
                project.classification == ChangeClassification.LikelyImprovement
            })
            .length()
            >= 2
    ) {
        ChangeClassification.LikelyImprovement
    } else {
        ChangeClassification.LikelyNeutral
    }
}

function generate_benchmark_report(
    head_manifest_path: string,
    output_path: string,
    baseline_manifest_path: string? = null,
) -> BenchmarkReport {
    let head = baml.json.from_string<BenchmarkManifest>(baml.fs.read(head_manifest_path));
    let selection = select_baseline(head, baseline_manifest_path);
    let baseline = selection.manifest;
    let projects = build_project_reports(head, baseline);
    let report = BenchmarkReport {
        schema_version: 1,
        classification: classify_projects(projects),
        baseline_status: selection.status,
        baseline_differences: selection.differences,
        base_skill_revision: if let base: BenchmarkManifest = baseline {
            base.skill_revision
        } else {
            null
        },
        head_skill_revision: head.skill_revision,
        suite_revision: head.suite_revision,
        model: head.model,
        projects: projects,
    };
    baml.fs.write(output_path, baml.json.stringify_pretty(baml.json.to_json(report)));
    report
}
