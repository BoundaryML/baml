// Data models for the feedback -> issue triage pipeline.

// ---------- Feedback ----------

enum FeedbackSource {
    BamlFeedback,
    Github,
    /// A report posted in the intake channel (slack.baml).
    Slack,
}

/// The reporter. At least one identity must be present — enforced by
/// `author_identity`, not by the type, so partially-filled records still parse.
class Author {
    email: string?,
    github: string?,
    device_id: string?,
}

class Comment {
    author: string,
    body: string,
    /// When it was written (RFC 3339); null on the oldest rows.
    at: string?,
    /// Where it came from: "github", "website", "slack"; null on the oldest rows.
    source: string?,
    /// Permalink to the original (a GitHub comment url); doubles as the
    /// de-duplication key for synced comments.
    url: string?,
}

class Feedback {
    id: string,
    title: string,
    body: string,
    source: FeedbackSource,
    author: Author,
    /// Toolchain version the reporter saw the bug on (e.g. "0.17.0"); null
    /// when the report does not say. Read it through `reporter_toolchain`.
    toolchain: string?,
    /// Files attached to the report (`baml feedback --files`), name -> content.
    files: map<string, string>,
    comments: Comment[],
    /// Issues already linked to this feedback (empty until triaged).
    issue_ids: string[],
}

/// The reporter's toolchain, or "unknown" when the report did not say.
function reporter_toolchain(fb: Feedback) -> string {
    fb.toolchain ?? "unknown"
}

class InvalidFeedback {
    message: string,
}

/// The identity we attribute the report to (email > github > device id).
function author_identity(a: Author) -> string throws InvalidFeedback {
    a.email ?? a.github ?? a.device_id
        ?? {
            throw InvalidFeedback { message: "feedback author has no identity" };
            ""
        }
}

// ---------- Issue ----------

enum Subsystem {
    Syntax,
    Compiler,
    Runtime,
    StdLibrary,
    Tooling,
    /// None of the above fits (or the defect spans several); routed to the
    /// default shepherds.
    Unknown,
}

/// How much work an issue is, as gauged from the ticket alone.
enum Difficulty {
    /// One obvious change: a typo, a message, a config default.
    Trivial,
    /// Localized fix in one place with a clear cause; under a day.
    Easy,
    /// A few days inside one subsystem; the cause is known or nearly so.
    Medium,
    /// Design or cross-subsystem work, or the root cause is still unknown.
    Hard,
}

// Status is a tagged union so each state carries exactly the payload the
// sketch calls for (reason / workaround / pr / version+date).
class Open {
    state: "open",
    /// New automatic work only; absent on the historical approval backlog.
    automatic_fix: bool?,
}

class InProgress {
    state: "in_progress",
    /// The draft PR the agent opened; null when a human took the issue.
    pr: string?,
}

class Rejected {
    state: "rejected",
    reason: string,
}

class Cancelled {
    state: "cancelled",
    reason: string,
    by: string,
}

class Deferred {
    state: "deferred",
    reason: string,
    workaround: string?,
}

class Merged {
    state: "merged",
    pr: string,
}

class Shipped {
    state: "shipped",
    version: string,
    date: string,
}

class AwaitingApproval {
    state: "awaiting_approval",
}

class Approved {
    state: "approved",
    by: string,
}

type IssueStatus = AwaitingApproval
    | Approved
    | Open
    | InProgress
    | Rejected
    | Cancelled
    | Deferred
    | Merged
    | Shipped;

/// What a report became. A feature request goes through the same pipeline
/// as a bug: its repro is the desired usage, verified to NOT work yet, and
/// `resolution_plan` holds the proposed feature instead of a fix plan.
enum IssueKind {
    Bug,
    Feature,
}

/// Null on rows written before feature requests existed: a bug.
function issue_kind(issue: Issue) -> IssueKind {
    issue.kind ?? IssueKind.Bug
}

class Issue {
    id: string,
    /// Bug or feature request; null means bug (see issue_kind).
    kind: IssueKind?,
    title: string,
    description: string,
    shepherd: string?,
    subsystem: Subsystem,
    /// An issue accumulates repros over its life; triage seeds the first one.
    repros: Repro[],
    /// Toolchain version the bug was last seen on: this host's when the
    /// repro reproduced it here, otherwise the reporter's (unverified).
    version: string,
    /// Feedback that this issue resolves.
    feedback_ids: string[],
    status: IssueStatus,
    comments: Comment[],
    /// Document outlining the fix; humans comment on it via `comments`.
    resolution_plan: string?,
    /// Null until gauged (gauge_issue).
    difficulty: Difficulty?,
    /// Hard issues only: suggested fix + design doc for the shepherd to hand
    /// to their own agent. Null until handle_issue runs the Hard path.
    design_doc: string?,
    /// The bot message to react to, bound to its Slack channel.
    slack_ts: string?,
    slack_channel: string?,
}
