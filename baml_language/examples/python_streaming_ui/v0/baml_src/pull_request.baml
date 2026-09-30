// v0 copy of baml_src/pull_request.prompt.baml; keep the two in sync.

class PullRequest {
    number int
    title string
    url string
    state "OPEN" | "CLOSED" | "MERGED"
    isDraft bool
    author User
    createdAt string
    mergedAt string?
    closedAt string?
    baseRefName string
    headRefName string
    labels Label[]
    additions int
    deletions int
    changedFiles int
    body string
    files ChangedFile[]
    commits Commit[]
    reviews Review[]
    comments Comment[]
}

class User {
    id string
    is_bot bool
    login string
    name string
}

class Actor {
    login string
}

class Label {
    name string
}

class ChangedFile {
    path string
    additions int
    deletions int
    changeType "ADDED" | "MODIFIED" | "DELETED" | "RENAMED" | "COPIED" | "CHANGED"
}

class Commit {
    authoredDate string
    authors CommitAuthor[]
    committedDate string
    messageBody string
    messageHeadline string
    oid string
}

class CommitAuthor {
    email string
    id string
    login string
    name string
}

class Review {
    id string
    author Actor
    authorAssociation string
    body string
    submittedAt string
    state "APPROVED" | "CHANGES_REQUESTED" | "COMMENTED" | "DISMISSED" | "PENDING"
    commit CommitRef
}

class CommitRef {
    oid string
}

class Comment {
    id string
    author Actor
    authorAssociation string
    body string
    createdAt string
    url string
}

function ExtractPullRequest(pr_text: string) -> PullRequest {
  client BedrockHaiku45
  prompt #"
    {{ _.role("user") }}
    Convert this GitHub pull request, written out as prose, back into structured data.
    Copy every text field (bodies, message bodies, headlines) verbatim, including markdown and HTML.
    Use null for timestamps given as "-". Enum-like values (state, changeType, authorAssociation) are upper-case.

    {{ pr_text }}

    {{ ctx.output_format }}
  "#
}
