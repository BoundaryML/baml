# atb2

The feedback pipeline: a user report becomes an issue, the issue becomes a
draft PR, the PR gets to green. Written in BAML against canary's toolchain
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
| handle  | `handle_issue.baml` | design pass, fix pass, the gate, a draft PR; a `runs` row and a thread reply |
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



## Current milestones and operation

See [MILESTONES.md](MILESTONES.md) for the workflow, quantitative goals, acceptance
criteria and known rollout prerequisites. The old approval flow is retired.

This issue-MVP layer ingests GitHub issues, CLI/PostHog feedback and signed Slack
mentions. It filters vague reports, verifies repros, deduplicates, organizes,
creates small draft fixes or larger investigation plans, and notifies shepherds.
PR monitoring is delivered by its own dependent stack. Shirts and scratch tasks
are not part of these milestone stacks.

### Runtime configuration

The existing app is `atb2-runner`, volume `atb2_data`. Build with
`docker build -f tools/atb2/deploy/Dockerfile tools/atb2` from the repository root.
Deployment uses `fly deploy tools/atb2 --config deploy/fly.toml`; never deploy an
unreviewed layer just to test a PR description. Canary changes deploy through
`.github/workflows/atb2-deploy.yml` when the app-scoped Fly token is configured.

Infisical supplies `FEEDBACK_SUPABASE_URL`, `FEEDBACK_SUPABASE_KEY`,
`ATB2_GITHUB_TOKEN`, Slack bot/signing credentials, PostHog credentials,
`ATB2_SHEPHERDS` (GitHub login:Slack member ID pairs), and `ATB2_UI_URL`.
`ATB2_ISSUE_CC` optionally adds a Slack member mention. Use a member ID, not a DM
channel ID. Issues go to the configured ATB channel. The default live channel is
set in `deploy/fly.toml`. Subscribe the app to `app_mention` and `reaction_added`
at `https://atb2-runner.fly.dev/slack/events` and invite it to that channel.

The root launcher obtains Infisical credentials and filters the runtime
environment. The builder UID cannot read the persistent Claude login. Agents
have isolated filesystems; the controller's trusted push path supplies GitHub
credentials only while pushing. Agents cannot merge or push to canary/main/master.
The CLI cache is shared and read-only in sandboxes; versions build on demand.

### Data and website

Apply the required SQL from the milestone PR descriptions in Supabase. No DDL
belongs in this repository. The current schema and session/turn tables are needed
before enabling intake; live and eval remain separated. Historical stopped runs
are not silently restarted, and historical approval rows are retained as history.

Only deploy the website to Vercel project `app-feedback`. Public pages use the
Supabase anonymous key. GitHub sign-in and private transcript access additionally
need `ATB2_GITHUB_CLIENT_ID`, `ATB2_GITHUB_CLIENT_SECRET`, `ATB2_UI_SESSION_SECRET`
and `ATB2_UI_RUNNER_SECRET`, with that last secret shared with the runner.
Do not assume a GitHub-linked Vercel project has canary as its production branch.
Verify that configuration before claiming automatic production deployments.

### Verification

Use the canary compiler: `baml check`, `baml fmt`, `baml test` in this package.
Run `python3 -m unittest discover -s tools/atb2/deploy -p 'test_*.py'` from the repo
root. Linux namespace tests require the runner image; a local skip is not a pass.
Security checks run before every controller push: secret scan, sensitive paths,
special/binary files, DDL, conflict markers, piped installers and action pinning.
CI on the PR is the build/test gate; an unmerged draft is an initial suggestion.


### Published feedback resolutions

The controller reconciles merged issue PRs against published stable releases. It
checks the actual merge SHA and release ancestry, then records the first verified
containing version. A PR merge alone never triggers an upgrade recommendation.
The companion CLI layer uses the bounded `feedback_resolutions` RPC described in
the release milestone PR. Apply that SQL in Supabase before enabling the client.
Nightly repro verification remains distinct from stable release indexing.
