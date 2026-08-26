// Data models for the feedback -> issue triage pipeline.

// ---------- Feedback ----------

enum FeedbackSource {
    BamlFeedback,
    Github,
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

// Status is a tagged union so each state carries exactly the payload the
// sketch calls for (reason / workaround / pr / version+date).
class Open {
    state: "open",
}

class InProgress {
    state: "in_progress",
}

class Rejected {
    state: "rejected",
    reason: string,
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

type IssueStatus = Open | InProgress | Rejected | Deferred | Merged | Shipped;

class Issue {
    id: string,
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
}
