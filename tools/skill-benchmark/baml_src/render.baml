function classification_label(value: ChangeClassification) -> string {
    match (value) {
        ChangeClassification.LikelyRegression => "Likely regression",
        ChangeClassification.LikelyNeutral => "Likely neutral",
        ChangeClassification.LikelyImprovement => "Likely improvement",
        ChangeClassification.BaselineUnavailable => "Baseline unavailable",
    }
}

function outcome_label(value: AttemptOutcome) -> string {
    match (value) {
        AttemptOutcome.Passed => "Passed",
        AttemptOutcome.Failed => "Failed",
        AttemptOutcome.TimedOut => "Timed out",
    }
}

function markdown_text(value: string) -> string {
    value.replace_all("|", "\\|").replace_all("\n", " ")
}

function one_decimal(value: float?) -> string {
    if let number: float = value {
        `${(number * 10.0).round() / 10.0}`
    } else {
        "-"
    }
}

function percent_change(head: float?, base: float?) -> string {
    if let head_value: float = head {
        if let base_value: float = base {
            if (base_value == 0.0) {
                "-"
            } else {
                let percentage = ((head_value - base_value) / base_value * 1000.0).round() / 10.0;
                if (percentage > 0.0) {
                    `+${percentage}%`
                } else {
                    `${percentage}%`
                }
            }
        } else {
            "-"
        }
    } else {
        "-"
    }
}

function report_links(path: string?) -> ReportLinks {
    if let links_path: string = path {
        if (baml.fs.exists(links_path)) {
            return baml.json.from_string<ReportLinks>(baml.fs.read(links_path));
        }
    }
    ReportLinks { attempts: [], evidence: [] }
}

function attempt_link(links: ReportLinks, attempt: AttemptReport) -> AttemptLink? {
    links.attempts.find((link) -> {
        link.attempt_id == attempt.id
    })
}

function evidence_url(links: ReportLinks, evidence: EvidenceRef) -> string? {
    links.evidence
        .find((link) -> {
            link.attempt_id == evidence.attempt_id && link.invocation_id == evidence.invocation_id
        })
        ?.url
}

function linked_label(label: string, url: string?) -> string {
    if (url) {
        `[${label}](${url})`
    } else {
        label
    }
}

function attempt_job_url(links: ReportLinks, attempt: AttemptReport) -> string? {
    attempt_link(links, attempt)?.job_url ?? attempt.artifacts.job_url
}

function attempt_artifact_url(links: ReportLinks, attempt: AttemptReport) -> string? {
    attempt_link(links, attempt)?.artifact_url ?? attempt.artifacts.transcript_url
}

function render_metric_row(name: string, head: float?, base: float?) -> string {
    `| ${name} | ${one_decimal(head)} | ${one_decimal(base)} | ${percent_change(head, base)} |`
}

function render_aggregate_table(head: AggregateMetrics, base: AggregateMetrics?) -> string {
    let base_attempts = base?.attempts ?? 0;
    let base_completed = base?.completed_attempts ?? 0;
    let rows = [
        "| Metric | This PR | Baseline | Change |",
        "|---|---:|---:|---:|",
        `| Completed attempts | ${head.completed_attempts}/${head.attempts} | ${if (base) { `${base_completed}/${base_attempts}` } else { "-" }} | - |`,
        render_metric_row("Agent turns", head.agent_turns_avg, base?.agent_turns_avg),
        render_metric_row("Tool calls", head.tool_calls_avg, base?.tool_calls_avg),
        render_metric_row("`baml check` failures", head.baml_check_failures_avg, base?.baml_check_failures_avg),
        render_metric_row("Implementation LOC", head.implementation_loc_avg, base?.implementation_loc_avg),
    ];
    rows.join("\n")
}

function render_attempt_table(attempts: AttemptReport[], links: ReportLinks) -> string {
    let rows = [
        "| Attempt | Result | Skill loaded | LOC | Turns | Tool calls | `baml check` |",
        "|---|---|---|---:|---:|---:|---:|",
    ];
    for (let attempt in attempts) {
        let label = linked_label(`${attempt.model} - attempt ${attempt.attempt}`, attempt_job_url(links, attempt));
        rows.push(
            `| ${label} | ${outcome_label(attempt.outcome)} | ${if (attempt.skill_loaded) { "Yes" } else { "No" }} | ${attempt.metrics.implementation_loc} | ${attempt.metrics.agent_turns ?? "-"} | ${attempt.metrics.tool_calls} | ${attempt.metrics.baml_check_calls} calls, ${attempt.metrics.baml_check_failures} failed |`,
        );
    }
    rows.join("\n")
}

function first_evidence_url(finding: Finding, links: ReportLinks) -> string? {
    if let evidence: EvidenceRef = finding.evidence.at(0) {
        evidence_url(links, evidence)
    } else {
        null
    }
}

function render_findings(
    attempts: AttemptReport[],
    links: ReportLinks,
    kind: FindingKind,
) -> string {
    let rows: string[] = [];
    for (let attempt in attempts) {
        for (let finding in attempt.findings) {
            if (finding.kind == kind) {
                let suggestion = if let change: string = finding.suggested_skill_change {
                    ` ${markdown_text(change)}`
                } else {
                    ""
                };
                let inspect = if let url: string = first_evidence_url(finding, links) {
                    ` [Inspect agent log](${url})`
                } else {
                    ""
                };
                rows.push(`- **${markdown_text(finding.title)}**${suggestion}${inspect}`);
            }
        }
    }
    if (rows.length() == 0) {
        "None identified."
    } else {
        rows.join("\n")
    }
}

function render_describes(attempts: AttemptReport[], links: ReportLinks) -> string {
    let sections: string[] = [];
    for (let attempt in attempts) {
        let rows: string[] = [];
        let index = 1;
        for (let describe in attempt.describes) {
            let command = markdown_text(describe.command.join(" "));
            let label = linked_label(`\`${command}\``, evidence_url(links, describe.evidence));
            rows.push(
                `${index}. ${label} - ${if (describe.succeeded) { "succeeded" } else { "failed" }}`,
            );
            index += 1;
        }
        sections.push(
            `#### ${attempt.model} - attempt ${attempt.attempt}\n\n${if (rows.length() == 0) { "No describe calls." } else { rows.join("\n") }}`,
        );
    }
    sections.join("\n\n")
}

function render_artifacts(attempts: AttemptReport[], links: ReportLinks) -> string {
    let rows = ["| Attempt | Run data |", "|---|---|"];
    for (let attempt in attempts) {
        let url = attempt_artifact_url(links, attempt);
        rows.push(
            `| ${attempt.model} - attempt ${attempt.attempt} | ${linked_label("Transcript, BAML audit, and generated code", url)} |`,
        );
    }
    rows.join("\n")
}

function render_project(project: ProjectReport, links: ReportLinks) -> string {
    `
        <details>
        <summary><strong>${markdown_text(project.name)}</strong> - ${classification_label(project.classification)}, ${project.head.completed_attempts}/${project.head.attempts} completed</summary>

        ### Aggregate results

        ${render_aggregate_table(project.head, project.base)}

        ### Attempts

        ${render_attempt_table(project.head_attempts, links)}

        ### Potential skill improvements

        ${render_findings(project.head_attempts, links, FindingKind.SkillGap)}

        ### Other implementation issues

        ${render_findings(project.head_attempts, links, FindingKind.ImplementationIssue)}

        ### \`baml describe\` usage

        ${render_describes(project.head_attempts, links)}

        ### Artifacts

        ${render_artifacts(project.head_attempts, links)}

        </details>
    `
}

function render_project_overview(projects: ProjectReport[]) -> string {
    let rows = ["| Project | Classification | Attempts |", "|---|---|---:|"];
    for (let project in projects) {
        rows.push(
            `| ${markdown_text(project.name)} | ${classification_label(project.classification)} | ${project.head.completed_attempts}/${project.head.attempts} completed |`,
        );
    }
    rows.join("\n")
}

function render_benchmark_report(report: BenchmarkReport, links: ReportLinks) -> string {
    let head_attempts = report.projects.flat_map((project) -> {
        project.head_attempts
    });
    let base_attempts = report.projects.flat_map((project) -> {
        project.base_attempts
    });
    let head = aggregate_attempts(head_attempts);
    let base = if (base_attempts.length() > 0) {
        aggregate_attempts(base_attempts)
    } else {
        null
    };
    let projects = report.projects
        .map((project) -> {
            render_project(project, links)
        })
        .join("\n\n");
    let baseline_message = match (report.baseline_status) {
        BaselineStatus.Available => {
            "Compared against the latest compatible successful canary artifact."
        },
        BaselineStatus.Missing => {
            "No canary baseline artifact was found. Results are shown without comparison and will become the baseline after merge."
        },
        BaselineStatus.Incompatible => {
            `A canary baseline exists, but ${report.baseline_differences.join(", ")} changed. Results are shown without comparison; canary will refresh it after merge.`
        },
    };
    `
        <!-- baml-skill-benchmark -->
        # BAML skill benchmark

        ## Overall result: ${classification_label(report.classification)}

        ${baseline_message}

        ${render_aggregate_table(head, base)}

        ${render_project_overview(report.projects)}

        ${projects}
    `
}

function render_report_file(
    report_path: string,
    output_path: string,
    links_path: string? = null,
) -> string {
    let report = baml.json.from_string<BenchmarkReport>(baml.fs.read(report_path));
    let markdown = render_benchmark_report(report, report_links(links_path));
    baml.fs.write(output_path, markdown);
    markdown
}
