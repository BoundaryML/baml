# agent-tries-baml

An event-driven benchmarking pipeline for BAML. It runs coding agents against
benchmark tasks (with the latest canary `baml` CLI on `PATH`), collects a verbose
self-reported "trophy" for each run, deduplicates the findings into a tracked
issue list, and dispatches fixes - all while a dashboard reads live state.

The system is three things:

- **Python services** - long-lived stateless workers (`baml-worker`,
  `baml-dedup`, `linear-sync`, `baml-builder`), the public `ingress` gateway,
  a `cron` driver, the `claude-proxy` agent runner, and a central `api` that is
  the only process allowed to talk to Convex.
- **Self-hosted Convex** - the database and queue substrate. Six tables
  (`tasks`, `trophies`, `issues`, `bamlBuilds`, `cohorts`, `workers`) plus
  `taskEvents`, each a *claimable queue* drained by exactly one worker at a time.
- **Next.js dashboard** (`ui`) - a read-only view of the pipeline that reads
  through the `api`.

Everything is organized around one symmetric idea: **every stage is a claimable
queue on a Convex table, drained by a long-lived `Processor` that talks only to
the central `api`.**

## Pipeline

![agent-tries-baml pipeline diagram](docs/pipeline.png)

Triggers (Slack `@mention`, cron, bug report, Linear approve) create `tasks`
through the API. `baml-worker` claims a task and writes a `trophy`; `baml-dedup`
merges its findings into `issues`; `linear-sync` (the board sync + fix
dispatcher) syncs issues to
Linear and dispatches Cursor cloud-agent fixes; `baml-builder` keeps the canary `baml`
binary registry fresh. The API is the only Convex client; agents reach Anthropic
only through `claude-proxy`; the Next.js UI reads pipeline state through the API.
See [`docs/architecture.md`](docs/architecture.md) for the full walkthrough.

### Skill arena

`@bot [skill arena] <task>` (optionally `[skill arena: branch-a, branch-b, …]`)
runs the same task once per **baml-skill branch** — a *cohort* of variant runs.
Each variant worker pulls its branch's `SKILL.md` and produces a **held** trophy
(not deduped). Once every member is terminal, the cohort reconciler (in `cron`)
flips it `queued`, and `cohort-compare` reads the variant runs, decides which
skill version served the task best, and emits a single comparison **cohort
trophy** — which re-enters `dedup → issues → Linear` like any other trophy, so a
winning variant's advantage becomes an actionable skill improvement. Branches
default to `ATB_ARENA_BRANCHES`. Issues keep their `kind` (`skill`/`language`) in
Convex; the Linear board is a single flat team (no kind split).

## Components

| Component | Role |
|---|---|
| `api` | The only Convex client. Central CRUD + queue verbs (claim / transition / heartbeat) + per-table SSE wake streams; stores transcript and baml-binary blobs on its own volume. |
| `claude-proxy` | Wraps `claude -p`; spawns the agent, parses the session into tokens/cost/turns, caches a `baml` binary per sha on `PATH`, and pre-installs the official skills via `baml agent install` on warm runs. |
| `baml-worker` | Claims `tasks`; runs the agent (official BAML skills installed via `baml agent install`, canary `baml` on `PATH`); the agent self-reports the whole verbose trophy; the worker verifies repros and creates the `trophy`. |
| `baml-dedup` | Claims `trophies`; authoritative skill/language classifier + cross-run merge; promotes findings/suggestions into `issues`; carries each cited finding's verified repro onto the issue. |
| `cohort-compare` | Claims ready `cohorts` (skill-arena groups); compares the variant runs (one per baml-skill branch) and emits a single comparison "cohort trophy" that re-enters dedup like any other trophy. |
| `baml-redraft` | Claims `issues` with `status=redraft`; pulls the reviewer's Linear comments as feedback, runs an agent to rewrite the issue, and re-boards it (`confirmed`) for another review pass. |
| `linear-sync` | Two processors over `issues`: `LinearPush` mirrors issues to Linear cards — title, status-group label, and a Markdown body incl. a repro code block (`linearSyncStatus` queue) — and `FixDispatch` dispatches `@cursor` fixes on approval (`status` queue). |
| `ingress` | Public webhooks: `/slack/events`, `/linear/webhook`, `/bug`. Creates `tasks`; reads an issue's Linear status-group label to route it to `approved` (fix) or `redraft`. |
| `cron` | Daily driver: refreshes baml (`POST /baml/update`) then enqueues benchmark `tasks`. Also runs the fast cohort fan-in reconciler that flips a skill-arena `cohort` `pending → queued` once its member runs are all terminal. |
| `baml-builder` | Claims `bamlBuilds`; downloads the prebuilt alpha-channel `baml` release binary for the sha and uploads it to the registry. |
| `ui` | Next.js dashboard; reads pipeline state through the `api`. |
| `convex` | Self-hosted Convex backend: schema, the generic claimable-queue library, per-table function modules, and the lease-reaper cron. |

## Uploading a local run

A run done locally with Claude Code (no proxy) can be pushed into the pipeline.
The `api` exposes `POST /ingest/run`, which parses a raw Claude Code session
`.jsonl` into a task + a queued trophy so the full `dedup → issues → Linear`
flow runs over it. The helper script reads the newest session under
`~/.claude/projects` and uploads it:

```bash
SERVICE_URL=http://localhost:8080 SERVICE_TOKEN=$ATB_SERVICE_TOKEN \
  python scripts/upload_local_run.py --prompt "what I asked the agent"
```

It prints the dashboard run URL. Pass `--session PATH` to upload a specific
session, `--baml-version SHA` to record the baml version, or `--trophy-json PATH`
to attach an agent self-report (summary / findings / filesCreated).

## Documentation

| Doc | What it covers |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | Runtime architecture: the claimable-queue pattern, the Processor claim loop, per-table state machines, and the lease reaper. |
| [`docs/data-model.md`](docs/data-model.md) | The Convex schema: every table, field, index, and lifecycle. |
| [`docs/configuration.md`](docs/configuration.md) | Environment variables and how config is injected (local `.env` / Infisical). |
| [`docs/reference.md`](docs/reference.md) | Consolidated API reference - every function/class with a one-line summary. |

## Layout

```
convex/           schema.ts  lib.ts  {tasks,trophies,issues,bamlBuilds}.ts  crons.ts  maintenance.ts
libs/bench_core/  processor  service_client  proxy_client  schemas  prices
                  slack_client  linear_client  cursor_client  jsonl
docker/           Dockerfile.python  Dockerfile.claude-proxy
```
