# atb2

The feedback pipeline: a user report becomes an issue, the issue becomes a
PR, the PR gets to green. Written in BAML against canary's language
(`~/.atb2/target/debug/baml-cli`, built by `handle_issue`; `BAML_CLI` overrides).

The runner builds the **latest published nightly** (`deploy/build-cli.sh`
reads pkg.boundaryml.com's nightly manifest and checks out its tag), so
every repro is confirmed on a toolchain reporters can install with
`baml toolchain use nightly`. New reports whose repro already passes make
no issue; open issues are re-run on every new nightly (`verify`) and ship
the moment they pass, which reporters see as "fixed in <nightly>" from
`baml feedback status`.

```
 baml feedback (PostHog)  ─┐
 Slack intake channel     ─┼─ ingest ─ triage ─ handle ─ merge
 GitHub issues            ─┘             │        │        │
                                      issues    runs   merge_rounds      (Supabase)
                                         └── one Slack thread per issue ──┘
```

| stage   | file                | what it does |
|---------|---------------------|--------------|
| ingest  | `intake.baml`, `slack.baml` | new reports from PostHog `baml_feedback` events and the Slack intake channel become `feedback` rows |
| triage  | `create_issue.baml`, `organize_issue.baml`, `gauge_issue.baml` | repro, ticket, shepherd, difficulty; an `issues` row and a Slack thread |
| handle  | `handle_issue.baml` | design pass, fix pass, the gate, a PR; a `runs` row and a thread reply |
| verify  | `verify_issue.baml` | open issues re-run on the runner's nightly; the ones that pass every repro become `shipped` with `fixed_in` set, and their thread hears |
| merge   | `merge_issue.baml`  | CI failures and reviewer comments back to `handle_issue` until the PR merges; `merge_rounds` rows |
| export  | `linear.baml`       | on demand: an issue becomes a Linear issue assigned to its shepherd, with the full description; re-exports update in place |
| intuit  | `intuition.baml`    | one Opus pass across every recent issue and every report that made none: what repeats or connects (one cause under several tickets, a subsystem breaking every nightly); `intuitions` rows, and an event on each cited issue. Skipped when no issue changed |

Every step that sets something on an issue records **why** (`decide` in
`store.baml`: step, decision, reason, evidence, as an `events` row). The
website renders those as the issue's decision trail and shows the reason
next to the subsystem, shepherd, difficulty and repro. The ticket itself is
written as a self-contained GitHub-style issue (environment, exact commands,
verbatim output, expected, what works, hypothesis, impact), not a summary.

A report becomes a **bug or a feature request** (`IssueKind`, the
`kind` column, `deploy/sql/issues_kind.sql`). Both go through the same
stages: a feature request's repro is the desired usage, verified to NOT
work on the latest nightly (one that already works is closed as
supported), its ticket is a proposal, and `resolution_plan` holds the
proposed feature instead of a fix plan. Both kinds go straight from triage
to the fix agent and a PR (a feature request's agent implements the
proposal and makes the desired-usage example pass); the website leads every
issue with which kind it is.

Comments on a reported GitHub issue are synced one way onto the report and
its issue every five minutes by the runner's GitHub poller
(`sync_github_comments`, keyed by the comment permalink); nothing is ever
written back to GitHub.

`pipeline.baml` runs them end to end; `store.baml` is the Supabase layer;
`models.baml` the shared types.

## Run

Every run needs the store, Slack and GitHub variables, which live in
Infisical (boundary-tools, prod): run `baml-cli` under `infisical run`, or
open a shell with them first.

```sh
cd tools/atb2
alias atb2='infisical run --projectId bdd280e2-259c-4750-9b16-a8597a67214c --env prod -- ~/.atb2/target/debug/baml-cli'
atb2 run -e 'run_pipeline()'                                   # everything, Live
atb2 run -e 'run_pipeline(mode = HandleMode.DryRun)'           # no push, no PR
atb2 run -e 'run_pipeline(stages = "ingest,triage")'           # a subset
atb2 run -e 'verify_issues()'                                  # re-run open issues on this nightly
atb2 run -e 'handle_issue(load_issue("ISSUE-…") ?? baml.sys.panic("no such issue"))'
atb2 run -e 'merge_issue("https://github.com/BoundaryML/baml/pull/4634")'
atb2 run -e 'export_issues_to_linear()'                        # open issues -> Linear
atb2 run -e 'export_issue_to_linear(load_issue("ISSUE-…") ?? baml.sys.panic("no such issue"))'
atb2 run -e 'run_intuition(force = true)'                       # rewrite the intuitions now
atb2 run -e 'sync_github_comments()'                            # pull GitHub comments once
```

Every stage is idempotent against the store; the intakes resume from a
cursor. Without the store the stages still run, and nothing is remembered.

## Wiring

All of these live in Infisical, project **boundary-tools** (`prod` has the
PostHog trio; `dev` does not). `run_tests.sh` and any live run go through
`infisical run --projectId bdd280e2-259c-4750-9b16-a8597a67214c --env prod -- …`.

| variable | used by | notes |
|----------|---------|-------|
| `FEEDBACK_SUPABASE_URL`, `FEEDBACK_SUPABASE_KEY` | store, evals | service key; the tables below exist in the project |
| `ATB2_SLACK_BOT_TOKEN`, `ATB2_SLACK_CHANNEL` | notifications | `chat:write`; one thread per issue; fall back to the old bot's `ATB_SLACK_BOT_TOKEN`, `ATB_SLACK_FIX_CHANNEL` |
| `ATB2_SLACK_INTAKE_CHANNEL` | Slack intake | `channels:history`; unset = no Slack intake |
| `ATB2_POSTHOG_API_KEY`, `ATB2_POSTHOG_PROJECT_ID`, `ATB2_POSTHOG_HOST` | PostHog intake | fall back to `ATB_POSTHOG_*`; personal key with `events:read`; host defaults to `https://us.posthog.com` |
| `ATB2_UI_URL` | notifications | links issues to `typescript2/app-feedback` |
| `ATB2_REVIEWERS`, `ATB2_POLL_S`, `ATB2_MAX_WAIT_S` | merge_issue | see `merge_issue.baml`; fork PRs are refused |
| `ATB2_MODEL`, `ATB2_HOME`, `ATB2_KEEP_RUNS`, `ATB2_MAX_TURNS` | handle_issue | see `handle_issue.baml`; `ATB2_MAX_TURNS` caps one agent session (design or fix pass), default 200, on top of the per-difficulty time budget |
| `ATB2_CANARY_REV`, `ATB2_TOOLCHAIN` | build-cli | unset = the latest nightly; a commit sha pins canary, `canary` tracks its head. `BAML_VERSION` overrides the version the pipeline records |
| `ATB2_LINEAR_API_KEY`, `ATB2_LINEAR_TEAM`, `ATB2_LINEAR_ASSIGNEES` | Linear export | see `linear.baml`; personal API key (falls back to existing `ATB_LINEAR_TOKEN`), team key or id, optional `github-login:linear-user-id` map (shepherds not mapped are matched by Linear display name, then email) |

The UI (`typescript2/app-feedback`) reads the same project with the anon
key: `FEEDBACK_SUPABASE_URL`, `FEEDBACK_SUPABASE_ANON_KEY` (server-only variables).

### Tables (project `igraichzcidsylvzkjlc`)

`feedback`, `issues`, `runs`, `merge_rounds`, `events`, `cursors`,
`intuitions`, one per model in `models.baml` / `handle_issue.baml` /
`merge_issue.baml` / `intuition.baml`, plus the views `issues_with_outcome`
(an issue with its latest run) and `feedback_public` (reports without the
reporter). Row level security: the anon role reads issues, runs,
merge_rounds, events, intuitions and the views; `feedback` and `cursors`
are service role only, so reporter identities never leave it. The DDL is
applied in the Supabase dashboard and is not in the repo, except
`deploy/sql/intuitions.sql`, which is the one table added after that rule
and must be applied before the intuit stage can write.

## Deploy

`tools/atb2/deploy` is the runner: a Fly app (`atb2-runner`), one machine
and one process, `runner_loop`, which serves `@bammy babysit <PR>` requests
from the store in the background and runs `run_pipeline` every five minutes.
One machine on purpose: a Fly volume attaches to one machine, and both loops
share the cached clone and cargo target on it. A panic ends the process and
Fly restarts it; every stage is idempotent against the store.

At boot the entrypoint builds the latest nightly's `baml-cli` on the volume
before any secret is loaded, in an explicit environment, and rebuilds
whenever the nightly moved (`ATB2_CANARY_REV` in `fly.toml` pins a canary
commit instead; `ATB2_TOOLCHAIN=canary` tracks canary's head). The version
lands in `/data/target/.baml-cli-version` for the pipeline to record. The image
carries cargo, gh, node and the Claude Code CLI; the volume at `/data` holds
the cached canary clone, the cargo target and the run dirs, and the first
boot builds canary's `baml-cli` there.

The image starts a root bootstrap that prepares volume permissions, then runs
compiler builds as `builder` (UID 1001) with a private home and cache under
`/data/bootstrap`. `/data/home` is mode 0700 and owned by `atb2` (UID 1000),
so build scripts cannot read the persistent Claude login. The builder receives
an allowlisted environment and no capabilities. Bootstrap copies the binary
using unprivileged readers/writers, fetches Infisical secrets as root, then
executes the privilege drop with a fresh, allowlisted environment. The machine
token never reaches the runtime process. The runtime has its own
Cargo cache under `/data/cargo`. Each user has a private Rustup installation
seeded from the image, so canary's required toolchains and components can be
installed independently. Shared tools and application files remain root-owned
and are not writable by either user.

The allowed runtime variables are listed in `deploy/launch-runtime.py`.
New runtime settings must be added there. Exported values are treated as
literal strings, with shell parameter expansion disabled.

Existing volumes keep their login and runtime state. The first boot after
this change builds a fresh compiler in the isolated cache, even if the old
runtime cache already contains one. Agent commands and gates additionally run inside Linux bubblewrap mount and PID
namespaces. Only their independent checkout and agent build cache are writable;
the controller HOME, Git metadata, processes and credentials are excluded.
Claude project hooks and project settings are disabled. A local broker forwards
only Claude message requests and keeps the persistent login outside the sandbox.
The runner refuses startup if namespace isolation is unavailable. Local agent
execution therefore requires the Linux runner image, rather than a host CLI.
Pushes export Git objects without credentials, then use fresh trusted metadata,
a fixed repository URL and an exact branch lease.

With the bammy GitHub App configured (`BAMMY_GITHUB_APP_CLIENT_ID` and
`BAMMY_GITHUB_APP_PRIVATE_KEY`, Infisical env `prod-atb2`, read alongside
`prod` via `INFISICAL_ENV = "prod,prod-atb2"`), the runner mints an
installation token per hour with `deploy/github-app-token.py`, every push,
PR and API call carries the App's bot identity, and no personal token
reaches the runtime. Without it, `ATB2_GITHUB_TOKEN` takes precedence over legacy GitHub token names. Live runs
check basic push access before starting the fix agent. This read-only check
cannot verify branch rules or workflow-file permissions: updating
`.github/workflows/` also requires the token's Workflows write permission.
Push failures preserve the local fix, record a terminal round outcome, and
report a safe failure category in Slack. Each push also writes `push-status.json`
in the private run directory with its phase and exit code, never raw Git output
or credentials. A failed push is not automatically retried: verify the remote
head and permission settings before requesting a new attempt.

Set `ATB2_CANARY_REV` to a commit SHA to pin the runner's compiler. If the
cached executable's recorded revision matches that pin, startup skips the
GitHub fetch and can proceed while GitHub is unavailable. Unpinned boots
and missing or mismatched cached builds still require a successful fetch.
The offline startup regression tests run with
`python3 tools/atb2/deploy/test_entrypoint.py`.

`.github/workflows/atb2-deploy.yml` redeploys it on every push to `canary`
that touches `tools/atb2`; it holds one secret, `FLY_API_TOKEN`, and skips
itself until that exists. The site (`typescript2/app-feedback`) deploys
through the Vercel GitHub app once its project is linked.

The runner's secrets come from Infisical at start: the image carries the
Infisical CLI. Configure the existing machine identity's `INFISICAL_CLIENT_ID`
and `INFISICAL_CLIENT_SECRET` as Fly secrets, as in the original baml-bench.
The root launcher logs in with Universal Auth on every boot, then captures
`infisical export --format=json` from boundary-tools `prod` in memory. Client
credentials are passed only in the login child's environment; the exporter
receives only the new token. None of these authentication credentials reaches
the builder or runtime user. Application-secret rotations take effect on restart.
An existing `INFISICAL_TOKEN` remains supported when neither client credential
is configured; a partial client pair fails closed. The runner's GitHub identity is `ATB_GITHUB_TOKEN` (or `GH_TOKEN` when set),
already in that project. The agent's Claude Code CLI runs on its own login,
made once on the machine (`fly ssh console -a atb2-runner`, then
`runuser -u atb2 -- env HOME=/data/home claude`)
and stored under `/data/home` on the runner's persistent volume. The Claude
credential is not stored in Infisical or CI and is not passed by atb2.

For a demo, `ATB2_INFISICAL_AUTH=user` explicitly selects a saved personal
Infisical CLI login instead of the configured machine credentials. Log in as
root with `HOME=/data/infisical-home` and the CLI's file vault. That directory
must be owned by root with mode `0700`; `/data` must be owned by root and not
writable by other users. The launcher validates these boundaries before reading
the login and still passes only allowlisted application settings to the runtime.
This mode uses the person's project access and may require a later interactive
login. Omit the setting to keep Universal Auth for unattended operation.

By hand, from the repo root:

```sh
fly apps create atb2-runner                                            # once (exists)
fly volumes create atb2_data --size 80 --region sjc -a atb2-runner     # once
# Stage INFISICAL_CLIENT_ID and INFISICAL_CLIENT_SECRET using:
# fly secrets import --stage -a atb2-runner
# Supply NAME=VALUE lines via stdin from your local secret store.
fly deploy tools/atb2 --config deploy/fly.toml
```

## Tests

```sh
tools/atb2/run_tests.sh wire       # store, slack, intake, pipeline: token-free
tools/atb2/run_tests.sh pr         # handle_issue, merge_issue token-free, then the agent evals
baml-cli test                      # everything token-free, no secrets needed
```

## Eval rows vs real rows

Every pipeline table carries `dataset`: `live` for real reports and what the
pipeline did with them, `eval` for anything written while `ATB2_DATASET=eval`,
which `run_tests.sh` exports for every stage it runs. The pipeline's own loops
(`load_issues_in`) only pick up rows of their own dataset, the UI badges eval
rows, and `dataset=eq.live` in a dashboard filter hides them.

The eval dataset itself (`eval/supabase`, tables `triage_issues` /
`triage_feedback`) is separate: reference issues and synthetic reports,
eval-only by construction.

### Slack Events intake

The Fly runner starts `main_ingress()`: HTTP on port 8080 shares the process
with the existing pipeline and merge-request loops. `/health` reports listener
liveness, not pipeline progress. `/slack/events` accepts signed Slack requests
within a five-minute replay window. `ATB_SLACK_SIGNING_SECRET` (or
`ATB2_SLACK_SIGNING_SECRET`) is loaded by the root launcher.

Explicit `babysit <PR URL>` mentions enter the shared babysitter queue. Other
mentions enter the durable session queue for questions, feedback, shirt claims,
or scratch tasks. A single store insert, capped at two seconds, precedes
HTTP acknowledgement. Unique `slack_event_id` values make retries harmless;
failed writes return 503 so Slack can retry. Slack acknowledgements are best
effort after persistence. The pipeline scans previously stored untriaged
reports, so work survives process restarts.

Before deployment apply the SQL from this branch's PR description. Configure
the Slack Events URL as `https://atb2-runner.fly.dev/slack/events` and subscribe
to `app_mention`. Use events instead of enabling the optional channel-history
poller for the same intake. `python3 deploy/test_slack_http.py` checks a real
local listener with fixture credentials and no external writes.

### Automatic fixes and Slack notifications

Newly triaged issues run automatically. Easy and Medium issues produce a draft
fix PR; Hard issues produce a design for a human. Existing paused issues are not
silently enrolled. Slack is a notification surface, not an approval gate.
The top-level message is the short title, issue link, and fix PR link. It begins
with "preparing a fix" and is updated when the draft exists. CI and CodeRabbit
follow-ups run in that issue's thread without waiting for reactions.

`ATB2_SHEPHERDS` maps GitHub logins to Slack member IDs. `ATB2_ISSUE_CC` is Sam's
verified Slack member ID (U...), included alongside the assigned shepherd.
A DM conversation ID (D...) is not accepted as a member mention.
Any mapped shepherd can react X to the issue anchor to cancel the issue and stop
its queued babysitting. New stage transitions and pushes check cancellation;
a process already computing may finish its current agent call before stopping.
Cancelled issues stay out of active listings, and GitHub PRs remain open.

Babysitting uses private controller-owned execution records on the Fly volume,
bound to the request, dataset and exact PR head. It does not create artificial
approval records or require approval-table schema changes. A stopped request
cannot authorize a later push, and failed executions are not replayed blindly.
The credential-bearing push helper refuses canary, main and master. No agent
merges, enables auto-merge, or marks a draft ready.

When only verified workflow configuration or infrastructure failures remain,
babysitting reports **Green, with workflow changes needed**. Missing evidence,
code/test failures and security issues are not workflow exemptions.

### Issue timeline and concise reports

New issue titles are at most 90 characters and descriptions at most 450.
Detailed repros, design notes and transcripts stay in their own sections.
The website collapses longer historical descriptions.
The timeline uses the stored feedback receipt time for ingestion and durable
stage events for enrichment, organization, difficulty, fix creation and Slack
delivery. Only successful Slack calls produce delivery events. Historical
stages without records are not assigned invented timestamps.

## Unified issue and PR workflow

PostHog `baml_feedback` events and Slack feedback mentions enter the same durable
feedback store. Triage calls `create_issue`, `organize_issue`, then `gauge_issue`.
A new issue is saved as automatic work and announced with its assigned shepherd.
The next handle pass claims it once, creates a draft fix PR without approval,
and reports the link immediately. It does not wait for CI or CodeRabbit before
posting the PR. Hard issues remain design-only.

Every PR created by `handle_issue` is immediately handed to `request_merge`.
An independent `@bammy babysit <PR URL>` enters exactly the same queue. All PR
observation, fix plans, round accounting, retries and request recovery
live in `merge_issue.baml`; `handle_issue` is the shared implementation/test/push
primitive, not a second review loop. Issue reconciliation can recover a missing
queue entry after a restart. Duplicate requests share one active babysitter.
The single worker coalesces concurrent intake races before running an agent.

Green and waiting-on-CI requests keep being observed for later comments until
merge/closure. Unchanged results do not repeat completion messages. Failed,
interrupted and round-limit results require human attention; stopped executions
are never replayed. The default round limit is seven. A fork PR is refused for modification.
The runner never auto-merges and cannot guarantee that an external review bot
will run or formally approve: that bot's repository settings still apply.

Issue pages show the lifecycle and link to `/prs/<number>` for the shared
babysitter timeline. Standalone babysit requests link there too. Proposal links
show the proposed fix summary while execution proceeds automatically. Public
event rows carry lifecycle metadata, safe summaries and proposal IDs, never the
private plan or CI log text. Pages refresh every 30 seconds. Slack outages do
not discard issue events; missing initial issue announcements are retried.

Offline queue integration checks: `python3 tools/atb2/deploy/test_workflow.py`
from the repository root. They run the real BAML runtime against a temporary
loopback store fixture; no model requests or production credentials are used.

## Activation checklist

1. Review and land the seven stacked PRs in order: runtime, issue intake,
   shared babysitter, release indexing, CLI notices, thread
   sessions, then scratch tasks/shirts.
2. Before deploying each layer, apply its SQL from the PR description in the
   Supabase dashboard. The session and scratch-task tables must exist before
   deploying parts 7a and 7b.
3. Configure the existing Infisical project/environment with the store service
   key, PostHog intake settings, GitHub token, Slack token/channel/signing secret,
   HTTPS `ATB2_UI_URL`, and `ATB2_SHEPHERDS` mappings for every assigned owner.
   Defaults are aaronvg (Syntax), codeshaunted (Compiler), antoniosarosi (Runtime),
   2kai2kai2 (StdLibrary), sxlijin (Tooling), and hellovai (Unknown). Missing maps
   cannot receive mentions or cancel issues. Keep `ATB2_SLACK_INTAKE_CHANNEL` unset when using Events intake.
4. Configure the feedback website's Supabase read credentials and Slack channel
   URL documented above. Deploy the website separately to its Vercel project.
   Runner Actions do not deploy the website.
5. Ensure Fly has the staged Infisical client pair and GitHub has an app-scoped
   `FLY_API_TOKEN`. Merge the reviewed PR into canary. The workflow deploys the
   runner automatically; a separate manual `fly deploy` is unnecessary when that
   workflow succeeds. The first compiler build may take 10–30 minutes.
6. Confirm the machine passes the namespace preflight and HTTP health check.
   SSH with `fly ssh console -a atb2-runner`, then log in with
   `runuser -u atb2 -- env HOME=/data/home claude`. Verify a broker-backed test run;
   live namespace/OAuth behavior has not been established by offline tests.
7. Set Slack Events URL to `https://atb2-runner.fly.dev/slack/events`, subscribe
   `app_mention` and `reaction_added`, add `reactions:read`, reinstall, and invite
   the bot to the issue/test channel. Existing bot scopes include `chat:write`
   and `app_mentions:read`.
8. Test a same-repository disposable PR with `@bammy babysit <PR URL>`. Check the
   linked UI, verify automatic fixes and push notifications, and confirm that
   later CI/review feedback is handled in the same thread without approval.
9. Submit one `baml feedback` report and confirm its PostHog event reaches the
   feedback store, then the issue page and Slack announcement. Verify automatic
   draft PR creation, shared babysitter activity and eventual manually merged
   status on both surfaces. Hard issues produce a design without implementation.

The CLI notice layers additionally need the two public repository variables
documented below and a published toolchain containing the CLI change. Their
release indexer verifies the first published stable release containing a fix;
merging a PR alone does not produce an update notice.

## Slack intake and notifications

The runner serves signed Slack events at `/slack/events` and health at `/health`
on port 8080. Subscribe to `app_mention` and `reaction_added`, with
`chat:write`, `app_mentions:read`, and `reactions:read`.
Set `ATB2_SHEPHERDS` to a comma-separated GitHub-login:Slack-user-ID map.
New issues mention their shepherd and begin the initial fix automatically.
Set `ATB2_ISSUE_CC` to an additional Slack member ID to mention that person too.
Slack reports the short title, issue link and draft fix PR link. No reaction
is needed to start or push a fix. Any configured shepherd can react with X to
cancel an issue; cancellation leaves the GitHub issue and PR open.

### Handoff and outgoing commits

The issue worker saves its result and closes the sandbox before reconciliation
queues its PR. Finished PRs retire unconsumed proposals; recovery preserves
queue timestamps so one request cannot repeatedly jump ahead of new work.
Before a trusted push, every outgoing commit is scanned with Infisical plus
checks for sensitive paths, credential patterns, special/binary files, DDL,
conflict markers, piped installers, and unpinned workflow actions. A scan failure
stops the push and requires human attention.

## Published feedback fixes

The hourly controller indexer backfills merge SHAs from GitHub for merged live
issues, then checks published stable `baml-language-X.Y.Z` tags and their package
manifests. It records the first verified containing release in `issues.fixed_in`.
Nightlies, drafts, unpublished manifests, and unverified ancestry never produce
update notices. The private Git index and polling lock live on the Fly volume.

Apply the part 6a SQL from its PR description before deployment.
`feedback_resolutions(report_ids uuid[])` accepts at most 100 full report UUIDs
and exposes only linked issue IDs, status and fixed version. Its lookup uses
feedback/issue primary keys; it does not expose reporter identities or contents.

### CLI resolution polling

`baml feedback status/list/view` refresh resolution data with a 1.5-second total
budget, falling back to the local cache when offline. Ordinary interactive
commands poll at most daily and print an update notice on stderr. After upgrading
to a stable toolchain containing the fix, the cached report displays `resolved`
and its reminder stops. Delivery state and anonymity remain separate.

Set repository variables `BAML_FEEDBACK_SUPABASE_URL` and
`BAML_FEEDBACK_PUBLISHABLE_KEY` before publishing the CLI. The latter must be a
public Supabase `sb_publishable_` key, never a service-role key. The toolchain
release workflow embeds these public values, including cross builds. Builds
without the publishable key leave polling inactive.

## Thread sessions

Slack mentions are durably inserted before acknowledgement. Explicit babysit
commands use the shared babysitter queue; other requests enter `agent_turns`.
The worker infers questions versus feedback, asks when ambiguous, and resumes
thread questions using `agent_sessions`. The session records Slack team/channel/
root thread, the Claude session UUID, latest checkout, and issue/PR/feedback links.
Session and turn tables are private to the service role.

Each session has a persistent isolated HOME on the Fly volume, separate from
`/data/home` and its login. A filesystem lock serializes agent turns, including
issue implementation and babysitter proposals. Resumed questions have only
Read/Glob/Grep; they cannot push changes. Each workflow
invocation gets an independent checkout; the last completed checkout is retained
for questions and replaced under the same session lock. Lost volume state starts
a replacement conversation with a Slack notice and stored context.

Apply the part 7a SQL before deploying. The worker is spawned inside the existing
runner process. The image uses digest-pinned Node 22 for the pinned Claude CLI.


## Scratch tasks and shirts

`@bammy try <task>` creates a scratch BAML project using the runner's canary
compiler. It resumes the thread's agent with Read/Write/Edit/Bash tools inside
the existing filesystem boundary. It never enters the push path. Follow-up
questions reuse that conversation with read-only tools. The runner saves a
private run containing the prompt, summary, visible text/tool turns, token count
when available, compiler revision, and any reproducible defects filed through
the shared feedback pipeline. Missing model credentials are reported as a
limitation; they are not filed as compiler defects.

The Slack reply includes the task summary. `/runs/[id]` and `/runs` direct users
back to Slack for follow-up questions; private transcripts remain in the store.
The raw Claude journal stays on the volume; initialization metadata and
thinking blocks are excluded from the stored transcript. Interrupted play runs
require a new mention rather than automatically repeating a task.

`@bammy give me a shirt` claims one code per Slack workspace/user through the
private `claim_promo` RPC and delivers it by DM. A retry returns the same claimed
code, so a failed DM does not consume a second one. Codes never appear in the
thread. Apply the part 7b SQL, then load codes through the Supabase dashboard.
Add the bot's `im:write` scope for `conversations.open`, reinstall it, and test DM delivery before inviting users to claim codes.

### Shared CLI versions and PR validation

On a cache miss, a credential-free builder publishes the requested `baml-cli` to
`/data/cli-cache/<version>/<source-revision>/baml-cli`. `index.json` lists the
available executables. The cache retains older versions/revisions and refuses
to overwrite an existing key with different bytes. Only root publishes this
cache; every sandbox sees it read-only. Versions are built lazily when an issue with repros requests them, from the
corresponding `baml-language-<version>` tag. If the bootstrap compiler already
matches, it is copied instead of rebuilt. Cache hits perform no build or fetch.
Agent-written executables never enter the shared cache. The currently installed CLI
remains at `/data/target/debug/baml-cli`.

Fix agents no longer start with a CLI rebuild, and the controller no longer
runs a local workspace test gate. Agents may run focused checks and must report
what they actually tested. The trusted push scans
outgoing commits, verifies the exact branch head, and pushes. Slack receives a
push notification; the babysitter then watches that PR's CI and reviewer
feedback and fixes subsequent failures automatically. A cached CLI
represents its recorded revision, never unbuilt edits in the checkout.

Before generating a repro, triage checks whether the feedback describes a concrete
problem or feature request. Vague reports stay in feedback storage with a terminal
`no_issue` event (`reason=needs_details`), without creating an issue or pinging a
shepherd. Specific reports do not need a complete repro to pass this check.

### Website comments, Linear export, and run transcripts

The existing final stack branch includes the website actions and runner API.
Issue pages remain public. Comments, Linear export, and play-session transcripts
require GitHub sign-in with active BoundaryML organization membership. Sessions
expire after one hour. The OAuth app uses only `read:org`; GitHub tokens are not
stored in browser cookies. Approval still happens in Slack.

Configure `app-feedback` with `ATB2_GITHUB_CLIENT_ID`,
`ATB2_GITHUB_CLIENT_SECRET`, `ATB2_UI_SESSION_SECRET` (a random secret of at least
32 characters), `ATB2_UI_URL` (the canonical HTTPS website origin), and
`ATB2_UI_RUNNER_SECRET` (a different random secret of at least 32 characters).
Set the OAuth callback to `<ATB2_UI_URL>/api/auth/github/callback`.
`ATB2_RUNNER_URL` defaults to `https://atb2-runner.fly.dev`.
Put the same `ATB2_UI_RUNNER_SECRET` in boundary-tools/prod for Fly. The existing
Linear token/team configuration is used by the runner, never the browser.

Play runs resolve the current canary commit at the beginning of each task. The
isolated builder lazily caches its CLI by version and exact revision. The agent
uses that executable's actual `feedback --anonymous` command; PostHog intake
continues to create the feedback rows. Run pages show the full visible session
(messages, tool calls/results, and follow-ups), report delivery state, and links
to the issues triage creates. Thinking and authentication metadata are excluded.
Transcripts refresh while the agent works and survive failed runs. Historical
runs that only saved truncated transcripts cannot recover missing data from the
old database snapshot alone.

`FEEDBACK_SUPABASE_ANON_KEY` must be present in boundary-tools/prod so each play
run can verify that the public role cannot read it. Before storing the prompt or
transcript, the worker reads the new, empty run with the anonymous role and
refuses to continue if it is visible. Supabase must hide `kind = 'play'` runs
from public readers. No schema migration is included in this repository.
CLI-reporting play runs are live-only; eval attempts are refused before any
external report can be sent.

Title deduplication is deterministic: normalize case/punctuation, remove common
filler words, normalize a few compiler terms, then match identical token sets or
Dice similarity of at least 0.60 with at least four shared terms. Candidates must
have the same subsystem and BAML version and remain open, awaiting approval,
approved, or in progress. A stable issue-ID tie-break handles older duplicates.
A match appends unique repros and feedback IDs without resetting approval/status,
then replies in the existing Slack thread. This is intentionally conservative;
it does not infer semantic equivalence or automatically consolidate old issues.

### Live agent transcripts

`/agents` lists real agent invocations; `/agents/<id>` polls new text, tool calls,
and tool results every two seconds. Issue pages link matching sessions, including
triage sessions matched through their feedback IDs. Deterministic organization
has no agent conversation; its existing lifecycle events remain on the issue.

The runtime captures both native BAML Claude clients (triage/gauging) and sandboxed
Claude processes (design/fix/babysit/try/chat). Records are stored privately on the
Fly volume under `/data/agent-traces`, outside agent mounts, and served through
the signed website bridge after GitHub organization sign-in. No new Supabase
table is required. Initialization/auth metadata and hidden reasoning are omitted.
Visible text and tool output are paged without truncation, including after errors.
A killed capture process is shown as interrupted. Capture begins with the updated
runner; it cannot reconstruct transcripts that older runners never recorded.

Try sessions have 75 turns and a 30-minute timeout; ordinary chat keeps its smaller
budget. The outer worker permits the longer try budget to finish.

### GitHub issue intake

A dedicated live-runner poller checks GitHub every minute and imports issues as
live `GHI-<number>` feedback,
with the full description, title, author and canonical link. The original GitHub
issue object is kept in origin metadata. PR entries are excluded. First startup
imports issues from the latest 100 repository entries; later scans resume from
a durable cursor, with five pages per pass. Repeated reads use insert-only writes
to preserve existing triage links. GitHub Issues read permission on the existing
runner token is required. Website OAuth is not used. This intake is disabled for
eval datasets. The poller persists reports even while implementation agents run;
the regular pipeline picks up those rows for filtering, deduplication and repros.
PostHog or Slack intake failures do not prevent stored reports from being triaged.
One-shot `run_pipeline` also polls GitHub, before the other intakes.

A completed babysitter investigation publishes its exact fix artifact and starts
implementation automatically. Transcript requests resolve the controller-assigned
fix ID, never the most recent event on a PR.
