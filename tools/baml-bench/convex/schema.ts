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
    bamlVersion: v.optional(v.string()), // sha of baml the run used
    // slack reply routing
    slackChannel: v.optional(v.string()),
    slackThreadTs: v.optional(v.string()),
    slackUser: v.optional(v.string()),
    notionProposerPageId: v.optional(v.string()),
    // worker output (transcriptStorageId is a pointer the service resolves
    // to a blob on its own volume — see services/api/blobs.py)
    transcriptStorageId: v.optional(v.string()),
    rawMetrics: v.optional(v.any()),
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_source", ["source"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  taskEvents: defineTable({
    taskId: v.id("tasks"),
    eventType: v.string(),
    details: v.optional(v.any()),
  }).index("by_task", ["taskId"]),

  // ---- trophies: the result DB ----
  trophies: defineTable({
    taskId: v.id("tasks"),
    outcome: v.string(), // success | partial | failed | quota_skipped
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
    suggestions: v.optional(v.array(v.any())), // {target, suggestion, rationale}
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_task", ["taskId"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- issues: the deduplicated issue DB ----
  issues: defineTable({
    kind: v.string(), // skill | language
    category: v.optional(v.string()), // bug | suggestion
    title: v.string(),
    description: v.string(),
    suggestion: v.optional(v.string()), // definitive skill/language fix
    // lifecycle: open -> confirmed -> approved -> fixing -> closed | rejected
    evidence: v.array(v.any()), // {trophyId, turnIndex?, callIndex?, note?}
    repro: v.optional(v.string()),
    notionPageId: v.optional(v.string()),
    fixSlackTs: v.optional(v.string()),
    firstSeenAt: v.number(),
    lastSeenAt: v.number(),
    notionSyncStatus: v.string(), // dirty | syncing | synced
    ...queueFields,
  })
    .index("by_status_created", ["status", "createdAt"])
    .index("by_kind_status", ["kind", "status"])
    .index("by_notion_sync", ["notionSyncStatus", "lastSeenAt"])
    .index("by_notion_page", ["notionPageId"])
    .index("by_lease", ["status", "leaseExpiresAt"]),

  // ---- bamlBuilds: version registry + build queue ----
  bamlBuilds: defineTable({
    sha: v.string(),
    ref: v.string(),
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
