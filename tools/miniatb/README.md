# MiniATB

A BAML implementation of ATB2's feedback-to-issue pipeline. Read `pipeline.baml`
first. The Fly configuration targets the existing `atb2-runner` app and `atb2_data` volume.

```text
Slack / PostHog / GitHub
  → durable queue → current nightly + worktree → JEV classification
  → Claude Code repro → independent replay → source investigation
  → concise ticket → investigation-based dedup → Supabase
  → fix → tests → secret scan → push → draft PR → Slack
```

A passing bug repro means **not reproduced**, not **already fixed**. Features are
investigated even when their example runs. Invalid or unsupported repros stop
without a ticket. Tickets contain a two-sentence description, four investigation
sentences with source links, and the native BAML repro with its execution output.

## Layout

`pipeline.baml` handles triage, repro, investigation and dedup; `fix_issue.baml`
creates draft fixes. `connections/` contains service integrations. `deployment/`
contains the queue, sandbox, publisher and Fly configuration. The website is
`typescript2/app-feedback`.

## Runtime boundaries

One bare repository supplies one worktree per report, on
`bammy/<run-id>-<report-slug>`. Repro and investigation see read-only source.
Only an explicit fix worker can write product source. The controller owns Git
metadata, commits and pushes. Agents cannot merge or invoke authenticated Git.

The only model tool is `shell`, implemented through Linux bubblewrap with its own
filesystem, empty HOME and network namespace. It exposes the current worktree,
scratch directory, basic tools and the verified BAML CLI. It does not expose other
runs, service secrets, Claude login or Git metadata. ClaudeCodeClient runs through
a launcher that disables native tools and strips service credentials; BAML agent
events carry the conversation between calls.

Every report resolves the latest nightly, validates the binary checksum and pins
source to the release SHA. Only the current toolchain is kept. The single worker
lock prevents toolchain changes during processing. The controller binary is
separate and updated when the container is rebuilt.

This sandbox does not provision every SDK, Rust dependency or model credential.
Those tests can be blocked and must be reported honestly. Native BAML repros are
the supported verification format.

## Configuration

| Variable | Purpose |
| --- | --- |
| `FEEDBACK_SUPABASE_URL`, `FEEDBACK_SUPABASE_KEY` | HTTPS store and controller service key. |
| `MINIATB_DATASET` | `eval` by default, or `live`; queues, cursors and transcripts use separate directories. |
| `TYPESAFE_API_KEY` | Optional JEV classification; absent means difficulty unknown. |
| `ATB2_POSTHOG_API_KEY`, `ATB2_POSTHOG_PROJECT_ID`, `ATB2_POSTHOG_HOST` | Enable PostHog polling. Existing `ATB_POSTHOG_*` aliases work. |
| `MINIATB_GITHUB_INGRESS=1` | Enable GitHub polling. Initial lookback is one hour, overridable with `MINIATB_GITHUB_SINCE`. |
| `BAMMY_GITHUB_APP_CLIENT_ID`, `BAMMY_GITHUB_APP_PRIVATE_KEY` | Mint repository-scoped installation tokens for intake and publishing. |
| `ATB_SLACK_SIGNING_SECRET`, `ATB_SLACK_FIX_CHANNEL` | Verify events and restrict ingress to the configured channel. |
| `ATB_SLACK_BOT_TOKEN`, `MINIATB_SLACK_NOTIFY=1` | Enable Slack notifications. Disabled by default. |
| `ATB2_SHEPHERDS` | Existing `github-login:SLACK_USER_ID,...` format. Routes by subsystem; only mapped users can cancel with X. |
| `ATB2_UI_URL`, `ATB2_UI_RUNNER_SECRET` | Website links and shared HMAC key, at least 32 characters. |
| `MINIATB_ALLOW_PUSH=1` | Permit publishing the automatic fix. Enabled in the Fly configuration; requires the Bammy GitHub App credentials. |
| `ATB2_LINEAR_API_KEY`, `ATB2_LINEAR_TEAM` | Optional website export; `ATB_LINEAR_TOKEN` remains a key alias. |

Claude's machine login lives in `MINIATB_CLAUDE_HOME` (the existing Fly volume uses `/data/home`; default `/data/claude`). Never put it in a repository.
Use the website's `.env.example` for its separate OAuth/session/anon-read settings.
The website never receives the Supabase service key or other controller tokens.

Slack mentions in the configured channel become feedback. Mapped shepherds can
cancel an issue by reacting X to its announcement. Cancellation is checked before
publishing; it does not interrupt an in-flight push.

PostHog and GitHub poll every 60 seconds with a five-minute overlap. GitHub PRs
and closed issues are excluded. Updates to queued reports do not start new runs.
Dedup compares the latest 100 unresolved issues in the same dataset, including
in-progress and deferred fixes. Duplicate reports attach to the existing ticket.

The first shepherd notification follows the fix attempt and includes the draft
PR or explains that no PR was produced. Failed fixes leave the ticket deferred.

## Verification and operation

From this directory, with the current compiler:

```sh
baml check --agent-skill-check off
baml fmt --agent-skill-check off
baml test --agent-skill-check off
```

Build from the repository root. The image compiles the controller from this
checkout and includes Rust with an offline dependency cache for fix verification.

```sh
docker build -t miniatb:local -f tools/miniatb/deployment/Dockerfile .
```

The explicit Linux sandbox checks need a disposable privileged container and do
not call models or external services:

```sh
docker run --rm --privileged -e MINIATB_LINUX_TEST=1 \
  -e MINIATB_RUN=0123456789abcdef01234567 miniatb:local \
  sh deployment/test-linux.sh
```

Run records live at `/data/miniatb/<dataset>/runs/<run-id>/`: original input,
pinned source/version, timestamped `timeline.jsonl`, controller logs, `issue.json`,
optional `fix.json`, worktree and scratch. Agent pages stream normalized BAML
events rather than a persisted Claude CLI session. Wire payloads are excluded.
Interrupted jobs require inspection rather than an automatic retry of side effects.

For an independently authorized fix of a saved ticket:

```sh
MINIATB_ALLOW_PUSH=1 baml run --agent-skill-check off fix -- --id <run-id>
```

Publishing runs the agent's tests, rebuilds the modified CLI offline, and replays
the original repro with that binary. Build-generated source changes are discarded
with the verification overlay. It then rejects protected
paths and binary artifacts, scans with controller-owned policy, and creates a
draft PR against canary.
A local publication journal reconciles existing commits, branches and PRs on retry.
Remote changes or closed PRs require inspection; retries never overwrite them.

Each shell command has aggregate limits of 12 GiB memory, 128 processes and two
CPU cores. Writable mounts share an 8 GiB tmpfs; completed commands may persist
up to 4 GiB scratch and 2 GiB source with at most 100,000 entries each. Ordinary
commands have five minutes, and controller builds have thirty. Limits require
Linux cgroups v1 or v2 and fail closed if unavailable. Completed run directories
still need operator retention management on the shared volume.

Linear exports use a per-issue lock plus an exact remote marker; an interrupted export's stale lock must be
inspected before removal.

## Not implemented

Standalone CI/CodeRabbit babysitting, try/chat sessions, shirts, release detection
and GitHub comment synchronization are not implemented. Historical UI pages remain
available for existing ATB2 records. Linear exports link to the reproduction and
do not map assignees. Model/service integration requires separate live testing.

## CLI update notices

`baml feedback status/list/view` refresh linked issue status with a 1.5-second
network budget and retain cached results offline. Ordinary interactive commands
poll and show update warnings at most once daily; quiet and piped invocations do
not poll. Once the installed version contains the fix, the cached status becomes
`resolved` and the warning stops. Nightly fixes recommend `baml toolchain use
nightly`; stable fixes recommend `baml toolchain update`.

The release workflow embeds repository variables `BAML_FEEDBACK_SUPABASE_URL`
and `BAML_FEEDBACK_PUBLISHABLE_KEY`. Only a public `sb_publishable_` key is accepted;
there is no service-role runtime fallback. Without this key, polling is inactive.
The existing `feedback_resolutions(report_ids uuid[])` RPC and issue `fixed_in`
metadata are required; this change does not apply SQL or run release detection.
Live PostHog reports retain `PH-<original-report-UUID>` IDs for that RPC. Eval
records remain separate and do not appear in users' resolution lookups.
