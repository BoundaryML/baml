// Mirrors the object `gh pr view --json <seed/make_seed.py FIELDS>` returns,
// field for field, so an extraction can be diffed against seed/pr-*.json.

class Actor {
  login: string
  name: string?
  id: string?
  is_bot: bool?
}

class CommitAuthor {
  login: string
  name: string
  email: string
  id: string
}

class Commit {
  oid: string
  messageHeadline: string
  messageBody: string
  authoredDate: string
  committedDate: string
  authors: CommitAuthor[]
}

enum ChangeType {
  ADDED
  MODIFIED
  DELETED
  RENAMED
  COPIED
  CHANGED
}

class ChangedFile {
  path: string
  additions: int
  deletions: int
  changeType: ChangeType
}

class Label {
  id: string
  name: string
  description: string?
  color: string
}

class ReactionUsers {
  totalCount: int
}

class ReactionGroup {
  content: string
  users: ReactionUsers
}

class Comment {
  id: string
  author: Actor
  authorAssociation: string
  body: string
  createdAt: string
  includesCreatedEdit: bool
  isMinimized: bool
  minimizedReason: string
  reactionGroups: ReactionGroup[]
  url: string
  viewerDidAuthor: bool
}

enum ReviewState {
  APPROVED
  CHANGES_REQUESTED
  COMMENTED
  DISMISSED
  PENDING
}

class CommitRef {
  oid: string
}

class Review {
  id: string
  author: Actor
  authorAssociation: string
  body: string
  submittedAt: string
  includesCreatedEdit: bool
  reactionGroups: ReactionGroup[]
  state: ReviewState
  commit: CommitRef
}

class ReviewRequest {
  login: string?
  name: string?
  slug: string?
}

enum PullRequestState {
  OPEN
  CLOSED
  MERGED
}

class PullRequest {
  number: int
  title: string
  url: string
  state: PullRequestState
  isDraft: bool
  author: Actor
  baseRefName: string
  headRefName: string
  createdAt: string
  updatedAt: string
  closedAt: string?
  mergedAt: string?
  mergedBy: Actor?
  mergeCommit: CommitRef?
  reviewDecision: ReviewState?
  additions: int
  deletions: int
  changedFiles: int
  labels: Label[]
  assignees: Actor[]
  reviewRequests: ReviewRequest[]
  body: string
  commits: Commit[]
  files: ChangedFile[]
  comments: Comment[]
  reviews: Review[]
}

// GC_BENCH_BASE_URL (unset: the provider's real API) is where bench.py points
// its replay or recording server.
client Haiku = anthropic.Client.new(
  model = "claude-haiku-4-5",
  base_url = env.GC_BENCH_BASE_URL,
  max_tokens = 32000,
  temperature = 0.0,
)

client Nano = openai.ResponsesClient.new(
  model = "gpt-5.4-nano",
  base_url = env.GC_BENCH_BASE_URL,
)

function _extraction_instructions() -> string {
  "You convert a plain-text description of a GitHub pull request back into the structured object it was written from. Copy every value verbatim, including full multi-line comment, review and commit bodies. Use null for absent values and [] for empty lists."
}

// The schema is rendered as TypeScript declarations (unquoted keys, `|`
// unions, string-literal enums, hoisted `interface`s) instead of the default
// JSON-ish rendering.
function ExtractPullRequest(text: string) -> PullRequest {
  client: Haiku
  prompt: `
    ${role("system")}
    ${_extraction_instructions()}

    ${ctx.output_format(
      prefix = "Answer with a JSON value matching this TypeScript type:\n",
      or_splitter = " | ",
      quote_class_fields = false,
      hoist_classes = true,
      hoisted_class_prefix = "interface",
      always_hoist_enums = false,
      enum_value_prefix = null,
      map_style = "object",
      render_null_as = "null",
    )}

    ${role("user")}
    ${text}
  `
}

// Same extraction on a comparably cheap OpenAI model (bench.py --provider openai).
function ExtractPullRequestNano(text: string) -> PullRequest {
  client: Nano
  prompt: `
    ${role("system")}
    ${_extraction_instructions()}

    ${ctx.output_format(
      prefix = "Answer with a JSON value matching this TypeScript type:\n",
      or_splitter = " | ",
      quote_class_fields = false,
      hoist_classes = true,
      hoisted_class_prefix = "interface",
      always_hoist_enums = false,
      enum_value_prefix = null,
      map_style = "object",
      render_null_as = "null",
    )}

    ${role("user")}
    ${text}
  `
}
