enum ChangeClassification {
    LikelyRegression,
    LikelyNeutral,
    LikelyImprovement,
    BaselineUnavailable,
}

enum Revision {
    Base,
    Head,
}

enum BaselineStatus {
    Available,
    Missing,
    Incompatible,
}

enum AttemptOutcome {
    Passed,
    Failed,
    TimedOut,
}

enum AuditStatus {
    running,
    exited,
    spawn_failed,
}

enum FindingKind {
    SkillGap,
    ImplementationIssue,
}

enum FailureCategory {
    Compilation,
    DescribeLookup,
    InterruptedCommand,
    ProcessFailure,
    Other,
}

class AgentCompletion {
    completed: bool,
    summary: string,
}

class AuditMetadata {
    schema_version: int,
    invocation_id: string,
    run_id: string,
    status: AuditStatus,
    shim_version: string,
    shim_pid: int,
    cwd: string,
    real_baml: string,
    argv: string[],
    subcommand: string?,
    started_at_unix_ms: int,
    finished_at_unix_ms: int?,
    duration_ms: int?,
    exit_code: int?,
    signal: int?,
    success: bool?,
    error: string?,
    stdout_path: string,
    stderr_path: string,
}

class EvidenceRef {
    attempt_id: string,
    invocation_id: string,
}

class DescribeCall {
    command: string[],
    succeeded: bool,
    evidence: EvidenceRef,
}

class FailedInvocation {
    invocation_id: string,
    command: string[],
    subcommand: string?,
    status: AuditStatus,
    exit_code: int?,
    stdout: string,
    stderr: string,
    later_success_same_command: bool,
    later_success_same_subcommand: bool,
}

class Finding {
    kind: FindingKind,
    category: FailureCategory,
    title: string,
    suggested_skill_change: string?,
    evidence: EvidenceRef[],
    resolved: bool?,
}

class FindingDraft {
    kind: FindingKind,
    category: FailureCategory,
    title: string @description(
        "One short sentence describing the issue without implementation-specific names that a reviewer would not recognize.",
    ),
    suggested_skill_change: string? @description(
        "One short, concrete documentation change when this is a skill gap. Null for ordinary implementation issues.",
    ),
    invocation_ids: string[],
    resolved: bool?,
}

class FindingDrafts {
    findings: FindingDraft[],
}

class ValidatedFindings {
    findings: Finding[],
    invalid_evidence_ids: string[],
}

class AttemptMetrics {
    implementation_loc: int,
    agent_turns: int?,
    tool_calls: int,
    baml_check_calls: int,
    baml_check_failures: int,
}

class AttemptArtifacts {
    job_url: string?,
    transcript_url: string?,
    audit_url: string?,
    implementation_url: string?,
}

class AttemptReport {
    schema_version: int,
    id: string,
    project_id: string,
    project_name: string,
    attempt: int,
    revision: Revision,
    model: string,
    skill_revision: string,
    skill_loaded: bool,
    outcome: AttemptOutcome,
    final_verification_passed: bool,
    metrics: AttemptMetrics,
    describes: DescribeCall[],
    findings: Finding[],
    invalid_evidence_ids: string[],
    artifacts: AttemptArtifacts,
}

class AggregateMetrics {
    attempts: int,
    completed_attempts: int,
    implementation_loc_avg: float?,
    agent_turns_avg: float?,
    tool_calls_avg: float?,
    baml_check_failures_avg: float?,
}

class ProjectReport {
    id: string,
    name: string,
    classification: ChangeClassification,
    base: AggregateMetrics?,
    head: AggregateMetrics,
    base_attempts: AttemptReport[],
    head_attempts: AttemptReport[],
}

class BenchmarkManifest {
    schema_version: int,
    generated_at: string,
    skill_revision: string,
    suite_revision: string,
    model: string,
    attempts_per_project: int,
    reports: AttemptReport[],
}

class BenchmarkReport {
    schema_version: int,
    classification: ChangeClassification,
    baseline_status: BaselineStatus,
    baseline_differences: string[],
    base_skill_revision: string?,
    head_skill_revision: string,
    suite_revision: string,
    model: string,
    projects: ProjectReport[],
}

class BaselineSelection {
    status: BaselineStatus,
    differences: string[],
    manifest: BenchmarkManifest?,
}

class AttemptLink {
    attempt_id: string,
    job_url: string,
    artifact_url: string,
}

class EvidenceLink {
    attempt_id: string,
    invocation_id: string,
    url: string,
}

class ReportLinks {
    attempts: AttemptLink[],
    evidence: EvidenceLink[],
}
