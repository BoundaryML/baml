# atb2

The feedback pipeline: a user report becomes an issue, the issue becomes a
PR, the PR gets to green. Written in BAML against canary's toolchain
(`~/.atb2/target/debug/baml-cli`, built by `handle_issue`; `BAML_CLI` overrides).

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
| handle  | `handle_issue.baml` | design pass, fix pass, secret scan, a PR; a `runs` row and a thread reply |
| merge   | `merge_issue.baml`  | CI failures and reviewer comments back to `handle_issue` until the PR merges; `merge_rounds` rows |

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
atb2 run -e 'handle_issue(load_issue("ISSUE-…") ?? baml.sys.panic("no such issue"))'
atb2 run -e 'merge_issue("https://github.com/BoundaryML/baml/pull/4634")'
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
| `ATB2_MODEL`, `ATB2_HOME`, `ATB2_KEEP_RUNS` | handle_issue | see `handle_issue.baml` |

The UI (`typescript2/app-feedback`) reads the same project with the anon
key: `FEEDBACK_SUPABASE_URL`, `FEEDBACK_SUPABASE_ANON_KEY` (server-only variables).

### Tables (project `igraichzcidsylvzkjlc`)

`feedback`, `issues`, `runs`, `merge_rounds`, `events`, `cursors`, one per
model in `models.baml` / `handle_issue.baml` / `merge_issue.baml`, plus the
views `issues_with_outcome` (an issue with its latest run) and
`feedback_public` (reports without the reporter). Row level security: the
anon role reads issues, runs, merge_rounds, events and the views; `feedback`
and `cursors` are service role only, so reporter identities never leave it.
The DDL is applied in the Supabase dashboard and is not in the repo.

## Deploy

`tools/atb2/deploy` is the runner: a Fly app (`atb2-runner`), one machine
and one process, `runner_loop`, which serves `@bammy babysit <PR>` requests
from the store in the background and runs `run_pipeline` every five minutes.
One machine on purpose: a Fly volume attaches to one machine, and both loops
share the cached clone and cargo target on it. A panic ends the process and
Fly restarts it; every stage is idempotent against the store.

At boot the entrypoint builds canary's `baml-cli` on the volume before any
secret is loaded, in an explicit environment, and rebuilds whenever the
canary revision changed (`ATB2_CANARY_REV` in `fly.toml` pins one). The image
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

Mentions route to `babysit <PR URL>`, a placeholder reply for shirt requests,
or durable feedback. A single store insert, capped at two seconds, precedes
HTTP acknowledgement. Unique `slack_event_id` values make retries harmless;
failed writes return 503 so Slack can retry. Slack acknowledgements are best
effort after persistence. The pipeline scans previously stored untriaged
reports, so work survives process restarts.

Before deployment apply the SQL from this branch's PR description. Configure
the Slack Events URL as `https://atb2-runner.fly.dev/slack/events` and subscribe
to `app_mention`. Use events instead of enabling the optional channel-history
poller for the same intake. `python3 deploy/test_slack_http.py` checks a real
local listener with fixture credentials and no external writes.

### Shepherd approval

Set `ATB2_SHEPHERDS` to explicit GitHub-login/Slack-user pairs, for example
`maintainer:U123,reviewer:U456`. Missing or ambiguous mappings cannot approve.
Subscribe the Slack app to `reaction_added`, add `reactions:read`, and reinstall
it. Apply this branch's schema additions before deploying.

Newly triaged issues wait in `awaiting_approval`. The mapped shepherd reacts
with `white_check_mark` or `+1` on the bot's approval message. Approval checks
the channel and exact message timestamp, then conditionally changes the state
to `approved`. The automatic handle stage reads approved issues only. Existing
open issues are moved into the approval workflow during triage; failed Slack
announcements are retried. Reopened issues get a fresh approval message.

Direct operator calls to `handle_issue` remain manual entrypoints. Babysitter
review rounds require the separate proposal approval described below. Neither
approval gate replaces the separate Linux filesystem boundary.


### Babysitter proposals and approval

`@bammy babysit <PR URL>` investigates CI failures and configured reviewer comments,
then publishes a proposed fix and test plan. The planning session has only Read,
Glob and Grep tools. The website shows the full proposal; Slack shows its summary
and a link. No implementation agent runs until this round is approved.

A thumbs up or check mark on the exact proposal message approves it when made by
a user explicitly mapped in `ATB2_SHEPHERDS`. Being the requester alone grants
no approval rights. Website approval requires GitHub
sign-in and current maintain/admin access to BoundaryML/baml. Proposals and CI
logs are private to those website users. Website authentication uses state, PKCE,
and an encrypted, secure HttpOnly cookie; repository access is checked again on
approval. No browser receives the store credential.

The runner consumes an approval once, implements its plan, scans outgoing commits
for secrets, announces the push in Slack, and pushes with a lease bound to the
approved PR head. If the head or feedback changed before execution, it creates a
new proposal. A concurrent push rejects the lease. A failed scan, blocked plan,
or interrupted execution never retries a consumed approval automatically; a human
can request babysitting again. New feedback after a successful push requires a new
approval. The final message reports checks and remaining configured-reviewer
feedback; it does not claim formal review approval or automatically merge the PR.
The runner no longer posts `@coderabbitai resolve` after a review round.

Pending proposals persist in `babysit_proposals` and release the worker. Approval
requeues their original request on the next poll. Direct `merge_issue` calls now
queue requests too, and the pipeline routes PR review work through the same queue.
Apply the additional proposal SQL from the PR description before starting this
version. It enables RLS with no anonymous access, makes proposal content immutable,
and restricts status transitions. The earlier Slack column SQL is not sufficient.

Set `ATB2_UI_URL` on the runner to the HTTPS feedback-site origin. For the website,
configure these server-only environment variables in addition to its read-only
store settings:

- `FEEDBACK_SITE_URL`: the HTTPS feedback-site origin.
- `FEEDBACK_GITHUB_CLIENT_ID` and `FEEDBACK_GITHUB_CLIENT_SECRET`: a GitHub OAuth
  app with callback `<FEEDBACK_SITE_URL>/auth/github/callback` and homepage equal
  to the site origin. Sign-in requests only `read:user`.
- `FEEDBACK_APPROVAL_SESSION_KEY`: 32 random bytes represented as 64 hex characters.
- `FEEDBACK_APPROVAL_SUPABASE_KEY`: a server-side store credential with access to
  the private proposals table (the Supabase service-role key is supported).

The website uses these credentials only in its authenticated server paths; its
existing public issue views continue to use the anonymous key. Do not place any
of these credentials in `NEXT_PUBLIC_` variables. Without website auth configured,
approval in Slack still works, but the private proposal page requires sign-in.

Checks: `bun test typescript2/app-feedback/tests/approval.test.mjs` verifies web
approval authorization, CSRF rejection, session tampering/expiry and one-shot writes.
The BAML tests cover proposal head/feedback binding and Slack approver/message checks.
Approval is a workflow control, not a fix for the existing agent HOME/filesystem
isolation limitation.

## Unified issue and PR workflow

PostHog `baml_feedback` events and Slack feedback mentions enter the same durable
feedback store. Triage calls `create_issue`, `organize_issue`, then `gauge_issue`.
A new issue is saved as `awaiting_approval` and announced in Slack with its website
link and assigned shepherd mention. The assigned shepherd's thumbs up or check
mark records approval and posts a reply. The next handle pass announces that it
is creating a fix, runs `handle_issue`, and creates a PR ready for review after
its tests pass. Hard issues produce a design document for a human instead.

Every PR created by `handle_issue` is immediately handed to `request_merge`.
An independent `@bammy babysit <PR URL>` enters exactly the same queue. All PR
observation, proposals, approval, round accounting, retries and request recovery
live in `merge_issue.baml`; `handle_issue` is the shared implementation/test/push
primitive, not a second review loop. Issue reconciliation can recover a missing
queue entry after a restart. Duplicate requests share one active babysitter.
The single worker coalesces concurrent intake races before running an agent.

Green and waiting-on-CI requests keep being observed for later comments until
merge/closure. Unchanged results do not repeat completion messages. Failed,
interrupted and round-limit results require human attention; approvals are never
replayed. The fixed round limit is three. A fork PR is refused for modification.
The runner never auto-merges and cannot guarantee that an external review bot
will run or formally approve: that bot's repository settings still apply.

Issue pages show the lifecycle and link to `/prs/<number>` for the shared
babysitter timeline. Standalone babysit requests link there too. Proposal links
lead to the existing maintainer-only plan/approval page. Public event rows carry
only lifecycle metadata and proposal IDs, never the private plan or CI log text.
Pages refresh every 30 seconds. Slack outages do not discard issue events;
unannounced issue approval messages are retried.

Offline queue integration checks: `python3 tools/atb2/deploy/test_workflow.py`
from the repository root. They run the real BAML runtime against a temporary
loopback store fixture; no model requests or production credentials are used.

## Activation checklist

1. Land the unified-workflow follow-up on the Slack PR. Before production use,
   finish the outgoing-agent-commit secret/content gate and the website dependency
   security updates identified in the review. Review CI and bot feedback.
2. Apply the Slack columns and additional proposal-table SQL from the PR body in
   the Supabase dashboard. This workflow follow-up needs no additional SQL.
3. Configure the existing Infisical project/environment with the store service
   key, PostHog intake settings, GitHub token, Slack token/channel/signing secret,
   HTTPS `ATB2_UI_URL`, and `ATB2_SHEPHERDS` mappings for every assigned owner.
   Defaults are aaronvg (Syntax), codeshaunted (Compiler), antoniosarosi (Runtime),
   2kai2kai2 (StdLibrary), sxlijin (Tooling), and hellovai (Unknown). Missing maps
   cannot approve. Keep `ATB2_SLACK_INTAKE_CHANNEL` unset when using Events intake.
4. Configure the feedback website's Supabase read credentials and GitHub OAuth
   approval credentials documented above. Set its production branch to canary in
   the linked Vercel project. Runner Actions do not deploy the website.
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
   linked UI, approve one proposed fix, verify tests and the pre-push Slack reply,
   and verify that later CI/review feedback requires another approval.
9. Submit one `baml feedback` report and confirm its PostHog event reaches the
   feedback store, then the issue page and Slack announcement. Have the assigned
   shepherd approve it. Verify fix creation, PR creation, shared babysitter
   activity and eventual manually merged status on both surfaces.

Part 6 is a separate follow-up PR: record the release version containing a fix,
map stored feedback IDs back to issues, and poll from ordinary CLI commands at
most daily. Show “Your issue … was fixed in …; update using baml toolchain update”
until the installed toolchain contains the fix. That notice is not required to
start babysitting or process feedback end to end, and is not implemented here.

## Slack intake and issue approval

The runner serves signed Slack events at `/slack/events` and health at `/health`
on port 8080. Subscribe to `app_mention` and `reaction_added`, with
`chat:write`, `app_mentions:read`, and `reactions:read`.
Set `ATB2_SHEPHERDS` to a comma-separated GitHub-login:Slack-user-ID map.
New issues announce their shepherd and wait for approval before implementation.

On the website the assigned shepherd can sign in with GitHub and click
**Approve issue**. Configure `FEEDBACK_SITE_URL`, `FEEDBACK_GITHUB_CLIENT_ID`,
`FEEDBACK_GITHUB_CLIENT_SECRET`, `FEEDBACK_APPROVAL_SESSION_KEY` (64 hex digits),
and server-only `FEEDBACK_APPROVAL_SUPABASE_KEY`. The OAuth callback is
`/auth/github/callback`. Slack and website approvals update the same pending row;
only the winning conditional write succeeds. The runner narrates website approvals
in the issue thread before starting the fix.

### Handoff and outgoing commits

The issue worker saves its result and closes the sandbox before reconciliation
queues its PR. Finished PRs retire unconsumed proposals; recovery preserves
queue timestamps so one request cannot repeatedly jump ahead of new work.
Before a trusted push, every outgoing commit is scanned with Infisical plus
checks for sensitive paths, credential patterns, special/binary files, DDL,
conflict markers, piped installers, and unpinned workflow actions. A scan failure
stops the push and requires human attention.

CLI versions are built only when an issue with repros needs one. The root-owned
cache service delegates builds to the credential-free builder UID and publishes
immutable executables under `/data/cli-cache/<version>/<revision>/baml-cli`.
Cache hits never build or fetch. Sandboxes mount this cache read-only.
PR CI is the build/test gate; fixes do not automatically rebuild the CLI or run
the full local workspace gate before pushing.
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
Read/Glob/Grep; they do not inherit an earlier push approval. Each workflow
invocation gets an independent checkout; the last completed checkout is retained
for questions and replaced under the same session lock. Lost volume state starts
a replacement conversation with a Slack notice and stored context.

Apply the part 7a SQL before deploying. The worker is spawned inside the existing
runner process. The image uses digest-pinned Node 22 for the pinned Claude CLI.
