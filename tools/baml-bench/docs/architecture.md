# Runtime architecture

baml-bench is built around a single pattern repeated at every stage of the
pipeline: a **claimable queue** on a Convex table, drained by a long-lived
**`Processor`** that speaks only to the central `api`. This document explains
that pattern, the shared claim loop, the per-table state machines, and the
lease reaper that keeps the whole thing self-healing.

## Pipeline at a glance

An event-driven, symmetric claimable-queue pipeline. Every box to the left of
the API is a `Processor` draining a Convex queue through the API; the API is the
only Convex client. Solid arrows are calls through the API; dashed flows
(`run agent`, alpha-binary download, Anthropic) cross a service boundary.

![baml-bench pipeline diagram](./pipeline.png)

*Event sources feed `ingress`/`cron`, which create tasks through the API. The API
is the sole Convex gateway; `baml-worker` and `baml-dedup` run agents through
`claude-proxy` (which reaches Anthropic and caches the baml binary per sha);
`baml-builder` pulls baml alpha releases from GitHub; `notion-fixer` syncs issues
to Notion and Slack; a read-only Next.js UI reads through the API. Convex holds
the `tasks`, `trophies`, `issues`, and `bamlBuilds` queues with their lifecycles.*

**Deployment shape:** always-on (min=1) `api`, `convex`, `claude-proxy`,
`ingress`; scale-out `baml-worker` (1..N), `baml-dedup` (batch=20),
`baml-builder` (0..1); the `ui` is min=0 / autostop.

**Processor claim loop:** (1) SSE `/events` wakes [30s poll backstop] -> (2)
`POST /claim` -> lease -> (3) heartbeat 60s -> (4) run work -> (5) `POST
/transition` -> (6) write next queue.

## The claimable-queue pattern

Every "claimable" Convex table (`tasks`, `trophies`, `issues`, `bamlBuilds`)
carries the same queue fields, spread in from `queueFields` in
[`convex/schema.ts`](../convex/schema.ts):

```text
status         per-table state machine (the claimable field)
claimedBy      worker id holding the current claim
claimedAt      when it was claimed
leaseExpiresAt claim expiry - the reaper requeues anything past this
attempts       claim count (drives max-attempts → failed)
lastError      last failure string
createdAt / updatedAt
```

A row is *claimable* when its claim field equals a configured value (e.g.
`status == "queued"`). Claiming is the one operation that must be
transactional: Convex mutations are serializable with optimistic concurrency
control (OCC), so among N racing workers exactly one flips a given row out of
the claimable value - no double-processing. That guarantee lives in
`claimDoc` in [`convex/lib.ts`](../convex/lib.ts), which:

1. queries the table's index for the **oldest** row with `field == value`,
2. patches it to the in-flight `claimedValue` and stamps `claimedBy`,
   `claimedAt`, `leaseExpiresAt = now + leaseMs`, and bumps `attempts`,
3. returns the claimed row (or `null` if the queue is empty).

Most tables are claimed on `status` via the `by_status_created` index. The
`issues` table is the exception - it has *two* independent queues (see below).

## The `Processor` base claim loop

All workers subclass `Processor` from
[`libs/bench_core/processor.py`](../libs/bench_core/processor.py). A subclass
declares what it consumes and implements `process(item)`; the base handles
everything else. Configuration lives as class attributes:

```python
table        = "tasks"      # the Convex table
claim_value  = "queued"     # the claimable value
claim_into   = "running"    # the in-flight value claim flips to
claim_field  = "status"     # the field claimed on (issues override this)
claim_index  = "by_status_created"
lease_ms     = 30 * 60 * 1000
heartbeat_secs    = 60.0
poll_backstop_secs = 30.0
batch        = False        # if True, process() drains the queue itself
```

The loop in `Processor.run()` has four moving parts:

- **SSE wake-ups.** The processor opens a long-lived Server-Sent-Events stream
  via `ServiceClient.events()` against `/{table}/events`. The `api` emits the
  current claimable count whenever it changes; each event triggers a `_drain()`.
  This makes the pipeline event-driven - work flows the instant a row becomes
  claimable, with no polling latency.
- **Atomic claim.** `_drain()` loops `_claim_one()` (which calls the service's
  `claim` verb → `claimDoc`) until the queue is empty, processing each claimed
  item. Because claiming is OCC-serializable, scaling to N workers is safe.
- **Heartbeat.** While the (potentially long) `process(item)` runs, a
  background task calls `heartbeat` every `heartbeat_secs`, pushing
  `leaseExpiresAt` forward so the reaper doesn't steal in-flight work.
- **Transition / fail.** On success, `process()` transitions the row to its
  terminal/next status. On an unhandled exception, `_run_one()` records
  `lastError` and transitions the row to `failed` **on the same field it was
  claimed on** - so failing a `notionSyncStatus` queue item never corrupts the
  issue's lifecycle `status`.
- **Poll backstop.** A separate `_poll_backstop()` task calls `_drain()` every
  `poll_backstop_secs` regardless of SSE. If the SSE stream drops, the loop
  logs and retries after 5s while the backstop keeps work moving. This makes
  SSE an optimization, not a correctness dependency.

On startup `run()` does an initial `_drain()` to catch up on anything already
queued before entering the SSE loop. `run_processor()` is the entry point: it
builds a `ServiceClient` from `SERVICE_URL` / `SERVICE_TOKEN` and runs one
processor to completion.

## Per-table state machines

State-machine *edges* are enforced in the Python services (the `api`), not in
Convex - `transitionDoc` simply sets the field. The reaper and the processors
agree on the in-flight values below.

### `tasks` - `queued → running → done`

A trigger (`ingress`/`cron`) creates a task as `queued`. `baml-worker` claims
it (`queued → running`), runs the agent through `claude-proxy`, creates a
`trophy`, and transitions the task to `done`. (A failed run is reaped from
`running` back to `queued`, or to `failed` after max attempts.)

### `trophies` - `queued → deduping → done`

`baml-worker` creates each trophy as `queued`. `baml-dedup` claims it
(`queued → deduping`), classifies and merges its findings into `issues`, then
transitions the trophy to `done`.

### `issues` - two independent queues

The lifecycle status:

```text
open → confirmed → approved → fixing → closed | rejected
```

`baml-dedup` upserts issues as `open`. The `notion-fixer` fix dispatcher claims
`status == "approved"` (`approved → fixing`) and dispatches an `@cursor` fix;
issues end at `closed` or `rejected`. A human (or Notion automation) is what
moves an issue toward `approved`; `ingress` `/notion/webhook` flips an issue to
`approved` when its Notion Status changes.

Running *alongside* the lifecycle is a **separate** Notion-sync queue on the
`notionSyncStatus` field:

```text
dirty → syncing → synced
```

`baml-dedup` marks an issue `notionSyncStatus = "dirty"` whenever it writes new
evidence. The `notion-fixer` push processor claims `notionSyncStatus == "dirty"`
(via the `by_notion_sync` index), creates or patches the Notion page, and sets
`notionSyncStatus = "synced"`. Keeping this on its own field/index means Notion
sync and the bug-fix lifecycle never block or corrupt each other - the same row
is claimable on two axes at once.

### `bamlBuilds` - `queued → building → ready | failed`

`cron` (or `POST /baml/update`) enqueues a build as `queued`. `baml-builder`
claims it (`queued → building`), downloads the prebuilt alpha-channel release
binary for that sha, uploads it, and transitions to `ready` on success or
`failed` on a download error. `ready` builds become the version cached on
`PATH` by `claude-proxy`.

## The lease reaper

A crashed processor would otherwise strand its in-flight rows forever. The
reaper in [`convex/maintenance.ts`](../convex/maintenance.ts), scheduled every
2 minutes by [`convex/crons.ts`](../convex/crons.ts), fixes this. For each rule
it scans the table's lease index for rows still in the in-flight value whose
`leaseExpiresAt` is in the past, and either **requeues** them or - if
`attempts >= MAX_ATTEMPTS` (4), and only for lifecycle `status` fields - moves
them to `failed`:

| Table | Field | In-flight value | Requeues to | Index |
|---|---|---|---|---|
| `tasks` | `status` | `running` | `queued` | `by_lease` |
| `trophies` | `status` | `deduping` | `queued` | `by_lease` |
| `issues` | `status` | `fixing` | `approved` | `by_lease` |
| `issues` | `notionSyncStatus` | `syncing` | `dirty` | `by_notion_sync` |
| `bamlBuilds` | `status` | `building` | `queued` | `by_lease` |

Because the heartbeat keeps pushing `leaseExpiresAt` forward during legitimate
long work, only genuinely dead claims get reaped. `maintenance.reap` is the
scheduled `internalMutation`; `maintenance.reapNow` is a public wrapper for ops
("force a reap now").
