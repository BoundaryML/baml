import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

// Shared queue/state-machine fields live on every "claimable" table.
// They are spread into each table def so tasks / trophies / issues /
// bamlBuilds are structurally identical where the generic queue logic
// touches them (see convex/lib.ts).
const queueFields = {
  status: v.string(), // per-table state machine; validated in lib.ts edges
  claimedBy: v.optional(v.string()),
  claimedAt: v.optional(v.number()),
  leaseExpiresAt: v.optional(v.number()),
  attempts: v.number(),
  lastError: v.optional(v.string()),
  createdAt: v.number(),
  updatedAt: v.number(),
};

export default defineSchema({
  // ---- tasks: the task/event DB ----
  tasks: defineTable({
    source: v.string(), // slack | cron | bug_report | pr_canary
    prompt: v.string(),
    repo: v.optional(v.string()),
    ref: v.optional(v.string()),
    sha: v.optional(v.string()),
    bamlVersion: v.optional(v.string()), // sha of baml the run used ("coldstart" for cold runs)
    // per-run mode directives (parsed from the slack message by ingress)
    bamlPin: v.optional(v.string()), // user-requested build selector (index, newest=1)
    bamlChannel: v.optional(v.string()), // nightly | canary (defaults to nightly)
    coldStart: v.optional(v.boolean()), // no prebuilt baml; agent installs from quickstart
    // skill-arena: a member task of a cohort runs the same prompt with a specific
    // baml-skill branch; the worker creates a held (cohort_member) trophy for it.
    cohortId: v.optional(v.string()), // the cohort this task is a member of
    skillRef: v.optional(v.string()), // baml-skill git branch this variant uses
    // slack reply routing
    slackChannel: v.optional(v.string()),
    slackThreadTs: v.optional(v.string()),
    slackUser: v.optional(v.string()),
    notionProposerPageId: v.optional(v.string()),
    // worker output (transcriptStorageId is a pointer the service resolves
    // to a blob on its own volume — see services/api/blobs.py)
    transcriptStorageId: v.optional(v.string()),
    // skill-arena: snapshot of the exact skill text this member run onboarded
    // from (a blob pointer), so the cohort page can show what each agent read.
    skillStorageId: v.optional(v.string()),
    rawMetrics: v.optional(v.any()),
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_source", ["source"])
    .index("by_cohort", ["cohortId"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  taskEvents: defineTable({
    taskId: v.id("tasks"),
    eventType: v.string(),
    details: v.optional(v.any()),
  }).index("by_task", ["taskId"]),

  // ---- trophies: the result DB ----
  trophies: defineTable({
    taskId: v.id("tasks"),
    source: v.optional(v.string()), // worker (default, absent) | feedback (baml feedback CLI / @bammy)
    outcome: v.string(), // success | partial | failed | quota_skipped | feedback
    compileOk: v.optional(v.boolean()),
    compileStderr: v.optional(v.string()),
    bamlVersion: v.optional(v.string()),
    metrics: v.any(), // full ported metric bag (see bench_core.schemas.Metrics)
    hostMetadata: v.optional(v.any()),
    transcriptStorageId: v.optional(v.string()),
    turnLog: v.optional(v.array(v.any())),
    // self-reported narrative (the worker agent writes the whole trophy)
    summary: v.optional(v.string()),
    whatWentWell: v.optional(v.array(v.string())),
    whatFailed: v.optional(v.array(v.string())),
    reportMd: v.optional(v.string()),
    findings: v.optional(v.array(v.any())), // {kind,title,description,anchor,suggestion?,repro?}
    filesCreated: v.optional(v.any()), // {path: content} of agent-authored project artifacts
    // which baml-skill this run onboarded from: skillUsed = repo URL, skillVersion =
    // commit sha. Together (repo@sha) they identify the exact skill, reproducibly.
    skillUsed: v.optional(v.string()),
    skillVersion: v.optional(v.string()),
    // Vestigial: legacy rows from the old reporter-agent design still carry this;
    // current code never sets it. Declared optional so existing trophies validate.
    reporterAgentId: v.optional(v.string()),
    suggestions: v.optional(v.array(v.any())), // {target, suggestion, rationale}
    // skill-arena: a member trophy carries cohortId and is held at status
    // "cohort_member" (never deduped); the synthesized comparison is a trophy with
    // isCohortReport=true that enters dedup like any other (status "queued").
    cohortId: v.optional(v.string()),
    isCohortReport: v.optional(v.boolean()),
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_task", ["taskId"])
    .index("by_cohort", ["cohortId"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- issues: the deduplicated issue DB ----
  issues: defineTable({
    kind: v.string(), // skill | language
    category: v.optional(v.string()), // bug | suggestion
    title: v.string(),
    description: v.string(),
    suggestion: v.optional(v.string()), // definitive skill/language fix
    // lifecycle: open -> confirmed -> approved -> dispatching -> tocursor ->
    //   prprep -> pr_ready -> closed | rejected | needs_human
    //   redraft -> redrafting -> confirmed (human-feedback redraft loop)
    // The cursor-tracker sweep (services/notion_fixer/tracker.py) drives
    // tocursor/prprep using the Cursor + GitHub fields below.
    evidence: v.array(v.any()), // {trophyId, turnIndex?, callIndex?, note?}
    repro: v.optional(v.string()),
    // ---- Linear board (the human-facing layer; Convex stays source of truth) ----
    linearIssueId: v.optional(v.string()), // the 1:1 Linear issue this mirrors to
    linearSyncStatus: v.optional(v.string()), // dirty | syncing | synced
    // ---- Notion board (DEPRECATED: replaced by Linear; kept optional, no migration) ----
    notionPageId: v.optional(v.string()),
    fixSlackTs: v.optional(v.string()), // FixDispatch idempotency marker / agent ref
    fixThreadTs: v.optional(v.string()), // Slack ts of the dispatch msg; tracker threads under it
    // ---- cursor-fix / PR tracking ----
    cursorAgentId: v.optional(v.string()), // the agent currently working the fix
    prUrl: v.optional(v.string()),
    prBranch: v.optional(v.string()),
    prNumber: v.optional(v.number()),
    fixAttempts: v.optional(v.number()), // auto fix-agent dispatches on this PR
    lastFixedSha: v.optional(v.string()), // PR head sha we last dispatched a fix for
    checkState: v.optional(v.string()), // pending | passing | failing (last observed)
    coderabbitState: v.optional(v.string()), // none | blocking | clear (last observed)
    // ISO created_at high-water mark of the newest human PR comment we've acted on,
    // so team-comment pickup is robust even when the 👀 reaction POST is forbidden.
    lastHumanCommentAt: v.optional(v.string()),
    // ---- bug-verify: re-checks against the newest nightly ----
    verifiedAt: v.optional(v.number()), // last re-check time
    verifyBamlVersion: v.optional(v.string()), // version label last checked against
    brokeIn: v.optional(v.string()), // version of the first evidence run
    fixedIn: v.optional(v.string()), // version where the repro stopped failing
    firstSeenAt: v.number(),
    lastSeenAt: v.number(),
    notionSyncStatus: v.optional(v.string()), // DEPRECATED (Linear replaced Notion)
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_kind_status", ["kind", "status"])
    .index("by_linear_sync", ["linearSyncStatus", "lastSeenAt"])
    .index("by_linear_issue", ["linearIssueId"])
    .index("by_notion_sync", ["notionSyncStatus", "lastSeenAt"])
    .index("by_notion_page", ["notionPageId"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- cohorts: skill-arena group (a "pod" of interconnected variant runs) ----
  // One cohort fans out into N member tasks (same prompt, different baml-skill
  // branch). A Python reconciler sweep (services/cron) flips the cohort
  // pending -> queued once every member task is terminal (done|failed); the
  // CohortCompare processor then claims it (queued -> comparing -> done) and emits
  // a single comparison "cohort trophy".
  cohorts: defineTable({
    prompt: v.string(),
    skillRefs: v.array(v.string()), // the baml-skill branches in this arena run
    memberTaskIds: v.array(v.string()), // the N member task ids (set by ingress)
    source: v.optional(v.string()), // slack | bug_report
    slackChannel: v.optional(v.string()),
    slackThreadTs: v.optional(v.string()),
    slackUser: v.optional(v.string()),
    reportTrophyId: v.optional(v.string()), // the cohort trophy, once compared
    // status: pending -> queued -> comparing -> done | failed
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- bamlBuilds: version registry + build queue ----
  bamlBuilds: defineTable({
    sha: v.string(),
    ref: v.string(),
    channel: v.optional(v.string()), // nightly | canary (derived from ref when absent)
    binaryStorageId: v.optional(v.string()),
    sizeBytes: v.optional(v.number()),
    contentHash: v.optional(v.string()),
    buildLogTail: v.optional(v.string()),
    builtAt: v.optional(v.number()),
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_sha", ["sha"])
    .index("by_ref_status", ["ref", "status"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- changelogEntries: changelog generation queue + published entries ----
  // One row per release tag. The cron poller enqueues missing versions
  // (status "queued"); the ChangelogWorker processor claims them
  // (queued -> generating -> done | failed) and writes title/body/meta on
  // completion. A revise/regenerate request flips a done row back to queued
  // with reviseMode/reviseGuidance set — the claim is the per-version lock.
  changelogEntries: defineTable({
    version: v.string(), // normalized (no baml-language- prefix); unique by convention
    tag: v.optional(v.string()), // full GitHub release tag
    channel: v.string(), // nightly | canary | alpha | engine | unknown
    date: v.optional(v.string()), // YYYY-MM-DD release date, set on completion
    title: v.optional(v.string()),
    body: v.optional(v.string()),
    authors: v.optional(v.array(v.string())),
    // generation inputs (consumed and cleared by the worker)
    fromRelease: v.optional(v.string()), // override predecessor tag for the diff
    reviseMode: v.optional(v.string()), // revise | regenerate
    reviseGuidance: v.optional(v.string()),
    // generation outputs (scores, attempts, code_checks, compared_against)
    meta: v.optional(v.any()),
    // slack reply routing for bammy-triggered edits
    slackChannel: v.optional(v.string()),
    slackThreadTs: v.optional(v.string()),
    slackUser: v.optional(v.string()),
    ...queueFields, // status: queued -> generating -> done | failed
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_version", ["version"])
    .index("by_channel", ["channel", "date"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- transcriptComments: human comments on run transcripts ----
  // Written from the dashboard (password-gated create). A comment is a
  // claimable queue row: dedup claims queued comments alongside its trophy
  // batch (queued -> deduping -> done) and turns them into issue evidence.
  transcriptComments: defineTable({
    trophyId: v.string(), // the run the comment is on
    taskId: v.optional(v.string()),
    turnIndex: v.optional(v.number()), // anchors to #turn-N; absent = run-level
    author: v.string(), // free-text display name
    body: v.string(),
    ...queueFields, // status: queued -> deduping -> done | failed
  })
    .index("by_trophy", ["trophyId", "createdAt"])
    .index("by_status_created", ["status", "createdAt"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- promoCodes: t-shirt promo code inventory ----
  // Not a claimable queue: a claim is one synchronous OCC mutation
  // (promoCodes.claimNext), so no lease machinery is needed.
  promoCodes: defineTable({
    code: v.string(),
    position: v.number(), // issue order; lowest unused position is claimed next
    status: v.string(), // unused | used
    claimedBy: v.optional(v.string()), // display name of the requesting Slack user
    claimedByUserId: v.optional(v.string()),
    notes: v.optional(v.string()),
    claimedAt: v.optional(v.number()),
    createdAt: v.number(),
    updatedAt: v.number(),
  })
    .index("by_status_position", ["status", "position"])
    .index("by_code", ["code"]),

  // ---- workers: presence / observability only ----
  workers: defineTable({
    workerId: v.string(),
    role: v.string(),
    status: v.string(), // idle | busy
    currentItemId: v.optional(v.string()),
    lastHeartbeat: v.number(),
  })
    .index("by_role_status", ["role", "status"])
    .index("by_worker", ["workerId"]),
});
