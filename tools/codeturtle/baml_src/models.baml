/// Whether a condition adds a human review requirement. This is not human approval.
enum Verdict {
    /// Require review by this rule's owners.
    Applies,
    /// Add no requirement; keep the negative result in the evaluation trace.
    DoesNotApply,
    /// Require human review because the condition could not confidently decide.
    Uncertain,
}

/// The human approval threshold of an ownership rule.
enum Approval {
    /// Any one of the listed owners can satisfy this requirement.
    Any,
    /// Every listed owner must sign off.
    All,
}

/// A changed file. Supported condition input: runs once per file matched by the rule.
/// Optional fields may be absent in saved fixtures; Git checks populate them.
class FileChange {
    /// Repository-relative current path; deleted files retain their original path.
    path: string,
    /// Added hunk lines joined with newlines, without the leading '+' markers.
    added_code: string,
    /// Previous repository-relative path for a rename; null otherwise.
    old_path: string?,
    /// Git name-status code, such as A, M, D or R100; null if not supplied.
    status: string?,
    /// Removed hunk lines joined with newlines, without the leading '-' markers.
    removed_code: string?,
    /// Full text at the merge-base; null for additions, binary content or absent fixture data.
    before: string?,
    /// Full text at head; null for deletions, binary content or absent fixture data.
    after: string?,
    /// Git patch with zero context lines, including file and hunk headers.
    diff: string?,
    /// True when either blob contains NUL bytes or cannot be decoded as UTF-8.
    is_binary: bool?,
    /// Changed hunk positions and lines; empty for binary files, null if not supplied.
    hunks: Hunk[]?,
}

/// One zero-context Git diff hunk. Positions are one-based; an empty side may start at zero.
class Hunk {
    /// First line position on the merge-base side.
    old_start: int,
    /// Number of old-side lines covered by the hunk; zero for a pure insertion.
    old_count: int,
    /// First line position on the head side.
    new_start: int,
    /// Number of new-side lines covered by the hunk; zero for a pure deletion.
    new_count: int,
    /// Inserted lines without '+' markers.
    added_lines: string[],
    /// Deleted lines without '-' markers.
    removed_lines: string[],
}

/// Immutable Git revisions used for this check; available through PullRequest.context.
/// This is metadata, not an independently injectable context or repository checkout.
class GitContext {
    /// Target revision resolved to a commit SHA. Trusted policy normally comes from here.
    base_revision: string,
    /// Commit SHA being reviewed.
    head_revision: string,
    /// Common ancestor used as the old side of the diff.
    merge_base: string,
    /// Commit SHA supplying trusted policy; null for an explicit local preview.
    policy_revision: string?,
    /// Either base_revision or local_preview.
    policy_source: string,
}

/// Supported condition input: runs once per rule with the title and every changed file.
/// Results may reference only files matched by that rule, even though all changes are visible.
class PullRequest {
    /// Explicit --title, otherwise the head commit subject; not fetched from GitHub.
    title: string,
    /// All changed files in the input, including files outside this ownership rule.
    changes: FileChange[],
    /// Git revision metadata; optional for saved fixtures.
    context: GitContext?,
}

/// A detailed condition result, returned alone for a file or in an array for files/PR input.
class Match {
    /// Exact path of a file matched by the rule; a file condition must use its input path.
    path: string,
    /// Applies and Uncertain require review; DoesNotApply does not.
    verdict: Verdict,
    /// Explain why this rule applies, does not apply, or is uncertain.
    reason: string,
    /// Supporting snippets or facts shown to the reviewer; may be empty.
    evidence: string[],
}

/// A parsed OWNERS rule. Rules accumulate; they do not override earlier matches.
class Rule {
    /// One-based line number in OWNERS.
    line: int,
    /// Supported patterns: '*', rooted exact file paths and rooted directory prefixes.
    pattern: string,
    /// GitHub-style owner handles beginning with '@'; identity membership is not verified locally.
    owners: string[],
    /// Any by default; All when the rule contains '# codeturtle: all'.
    approval: Approval,
    /// Nested BAML function name from when=; null for unconditional ownership.
    condition: string?,
}

/// One evaluated result, including negative matches, with its policy provenance.
class Evaluation {
    /// One-based ownership rule line number.
    rule_line: int,
    /// Invoked condition name; null for an unconditional path rule.
    condition: string?,
    /// Normalized detailed result; boolean conditions get a generated reason and empty evidence.
    result: Match,
}

/// A human review requirement produced by an Applies or Uncertain result.
class Requirement {
    /// File requiring review.
    path: string,
    /// Owners assigned by this rule.
    owners: string[],
    /// Required owner approval threshold.
    approval: Approval,
    /// The rule, reason and evidence that created this requirement.
    evaluation: Evaluation,
}

/// A blocking policy or execution error. Errors are never treated as false conditions.
class EvaluationError {
    /// Affected OWNERS line, or null for package/input failures.
    rule_line: int?,
    /// Diagnostic message; JSON preserves full compiler diagnostics.
    message: string,
}

/// Typed codeturtle check JSON output. It describes evaluation, not collected human sign-offs.
class Report {
    /// Revision metadata, or null for fixtures and early failures.
    context: GitContext?,
    /// Accumulated review requirements from all successful rules.
    requirements: Requirement[],
    /// Evaluated positive, negative and uncertain matches.
    evaluations: Evaluation[],
    /// Blocking failures; successful other rules are retained.
    errors: EvaluationError[],
    /// Changed files without any review requirement. A false predicate does not waive coverage.
    unowned_files: string[],
    /// True only when errors and unowned_files are empty; never means humans have approved.
    evaluation_complete: bool,
}

/// A referenced condition validated by codeturtle lint without invoking it.
class ConditionInfo {
    /// User-facing nested BAML name, without the internal root. prefix.
    name: string,
    /// Inferred invocation scope: file, files or pr.
    scope: string,
}

/// Typed codeturtle lint JSON output. Compilation may initialize the package; predicates are not called.
class LintReport {
    /// True when parsing, compilation and every referenced function contract succeed.
    valid: bool,
    /// Number of parsed ownership rules.
    rule_count: int,
    /// Number of BAML source files compiled.
    source_count: int,
    /// Unique referenced conditions and their inferred scopes.
    conditions: ConditionInfo[],
    /// Syntax, compilation and contract failures.
    errors: EvaluationError[],
}

type FileCondition = (FileChange) -> (Match | bool) throws unknown;

type FilesCondition = (FileChange[]) -> (Match[] | bool) throws unknown;

type PrCondition = (PullRequest) -> (Match[] | bool) throws unknown;

/// (internal) One scheduled condition invocation.
class Job {
    rule: Rule,
    changes: FileChange[],
}

/// (internal) Rule-level collection and result validation state.
class RuleBatch {
    rule: Rule,
    allowed: string[],
    indices: int[],
}

/// (internal) A completed invocation or captured exception.
class JobResult {
    matches: Match[],
    error: string?,
}

/// (internal) One reflected field or enum variant in the agent help payload.
class HelpMember {
    name: string,
    type: string?,
    description: string?,
}

/// (internal) An automatically discovered public schema declaration.
class HelpType {
    name: string,
    kind: string,
    description: string?,
    members: HelpMember[],
}

/// (internal) A reflected callable contract plus its invocation guidance.
class HelpContract {
    name: string,
    input: string,
    returns: string,
    invocation: string,
}

/// (internal) Reflected CodeTurtle API documentation, rendered by the CLI.
class HelpReport {
    guidance: string,
    types: HelpType[],
    contracts: HelpContract[],
}
