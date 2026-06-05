# Data model

The Convex schema lives in [`convex/schema.ts`](../convex/schema.ts). Five
tables (`tasks`, `trophies`, `issues`, `bamlBuilds`, `workers`) plus the
append-only `taskEvents` log. The four queue tables share a common
`queueFields` block; `workers` is presence/observability only.

## Shared queue fields

Spread into `tasks`, `trophies`, `issues`, and `bamlBuilds` so the generic
claimable-queue logic in [`convex/lib.ts`](../convex/lib.ts) can treat them
uniformly:

| Field | Type | Purpose |
|---|---|---|
| `status` | `string` | Per-table state machine (the claimable field). |
| `claimedBy` | `string?` | Worker id holding the current claim. |
| `claimedAt` | `number?` | When the row was claimed. |
| `leaseExpiresAt` | `number?` | Claim expiry; the reaper requeues anything past it. |
| `attempts` | `number` | Claim count; drives max-attempts → `failed`. |
| `lastError` | `string?` | Last failure string. |
| `createdAt` | `number` | Insert time. |
| `updatedAt` | `number` | Last mutation time. |

## `tasks` - the task / event DB

A unit of benchmark work: a prompt to run with the canary baml on `PATH`.

| Field | Type | Notes |
|---|---|---|
| `source` | `string` | `slack` \| `cron` \| `bug_report`. |
| `prompt` | `string` | The agent prompt. |
| `repo` | `string?` | Target repo. |
| `ref` | `string?` | Git ref. |
| `sha` | `string?` | Git sha. |
| `bamlVersion` | `string?` | Sha of the baml CLI the run used. |
| `slackChannel` | `string?` | Slack reply routing. |
| `slackThreadTs` | `string?` | Slack reply routing. |
| `slackUser` | `string?` | Slack reply routing. |
| `notionProposerPageId` | `string?` | Notion page that proposed the task. |
| `transcriptStorageId` | `string?` | Pointer the `api` resolves to a transcript blob on its own volume. |
| `rawMetrics` | `any?` | Raw run metrics. |
| *(+ shared queue fields)* | | |

**Indexes:** `by_status_created` (`status`, `createdAt`) · `by_source`
(`source`) · `by_lease` (`status`, `leaseExpiresAt`).

**Lifecycle:** `queued → running → done` (reaped `running → queued`, or
`→ failed` after max attempts).

## `taskEvents` - append-only task audit log

Written automatically by `createDoc`, `claimDoc`, and `transitionDoc` whenever
the table is `tasks`.

| Field | Type | Notes |
|---|---|---|
| `taskId` | `id("tasks")` | The task this event belongs to. |
| `eventType` | `string` | The status / lifecycle event (e.g. `created`, `running`, `done`). |
| `details` | `any?` | Optional payload (e.g. `{ workerId }` on claim). |

**Index:** `by_task` (`taskId`).

## `trophies` - the result DB

The verbose self-reported outcome of one task run.

| Field | Type | Notes |
|---|---|---|
| `taskId` | `id("tasks")` | The task that produced this trophy. |
| `outcome` | `string` | `success` \| `partial` \| `failed` \| `quota_skipped`. |
| `compileOk` | `boolean?` | Whether the produced baml compiled. |
| `compileStderr` | `string?` | Compiler stderr. |
| `bamlVersion` | `string?` | Sha of the baml CLI used. |
| `metrics` | `any` | Full ported metric bag (see `bench_core.schemas.Metrics`). |
| `hostMetadata` | `any?` | Runner host info. |
| `transcriptStorageId` | `string?` | Transcript blob pointer. |
| `turnLog` | `array(any)?` | Per-turn log. |
| `summary` | `string?` | Self-reported narrative summary. |
| `whatWentWell` | `array(string)?` | Self-reported positives. |
| `whatFailed` | `array(string)?` | Self-reported failures. |
| `reportMd` | `string?` | Rendered markdown report. |
| `findings` | `array(any)?` | `{kind, title, description, anchor, suggestion?, repro?}`. |
| `suggestions` | `array(any)?` | `{target, suggestion, rationale}`. |
| *(+ shared queue fields)* | | |

**Indexes:** `by_status_created` (`status`, `createdAt`) · `by_task`
(`taskId`) · `by_lease` (`status`, `leaseExpiresAt`).

**Lifecycle:** `queued → deduping → done`.

## `issues` - the deduplicated issue DB

A cross-run merged skill/language issue, with two independent queues.

| Field | Type | Notes |
|---|---|---|
| `kind` | `string` | `skill` \| `language`. |
| `category` | `string?` | `bug` \| `suggestion`. |
| `title` | `string` | Issue title. |
| `description` | `string` | Issue description. |
| `suggestion` | `string?` | Definitive skill/language fix. |
| `evidence` | `array(any)` | `{trophyId, turnIndex?, callIndex?, note?}` anchors. |
| `repro` | `string?` | Minimal repro. |
| `notionPageId` | `string?` | Linked Notion page id. |
| `fixSlackTs` | `string?` | Dispatch reference for the fix (the Cursor cloud-agent id). |
| `firstSeenAt` | `number` | First observation time. |
| `lastSeenAt` | `number` | Most recent observation time. |
| `notionSyncStatus` | `string` | `dirty` \| `syncing` \| `synced` (separate sync queue). |
| *(+ shared queue fields)* | | |

**Indexes:** `by_status_created` (`status`, `createdAt`) · `by_kind_status`
(`kind`, `status`) · `by_notion_sync` (`notionSyncStatus`, `lastSeenAt`) ·
`by_notion_page` (`notionPageId`) · `by_lease` (`status`, `leaseExpiresAt`).

**Lifecycles:**
- Bug-fix: `open → confirmed → approved → fixing → closed | rejected`.
- Notion sync (`notionSyncStatus`): `dirty → syncing → synced`.

## `bamlBuilds` - version registry + build queue

One row per attempted build of the `baml` CLI.

| Field | Type | Notes |
|---|---|---|
| `sha` | `string` | Source sha being built. |
| `ref` | `string` | Source ref. |
| `binaryStorageId` | `string?` | Pointer to the uploaded binary blob. |
| `sizeBytes` | `number?` | Binary size. |
| `contentHash` | `string?` | Hash of the binary. |
| `buildLogTail` | `string?` | Tail of the build log. |
| `builtAt` | `number?` | Completion time. |
| *(+ shared queue fields)* | | |

**Indexes:** `by_status_created` (`status`, `createdAt`) · `by_sha` (`sha`) ·
`by_ref_status` (`ref`, `status`) · `by_lease` (`status`, `leaseExpiresAt`).

**Lifecycle:** `queued → building → ready | failed`.

## `workers` - presence / observability

Not a queue. Tracks live processors for the dashboard.

| Field | Type | Notes |
|---|---|---|
| `workerId` | `string` | Worker identity. |
| `role` | `string` | Processor role (e.g. `baml-worker`). |
| `status` | `string` | `idle` \| `busy`. |
| `currentItemId` | `string?` | Id of the item currently being processed. |
| `lastHeartbeat` | `number` | Last presence ping. |

**Indexes:** `by_role_status` (`role`, `status`) · `by_worker` (`workerId`).
