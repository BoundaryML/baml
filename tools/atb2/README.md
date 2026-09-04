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
Infisical CLI and the entrypoint wraps the process in `infisical run` against
boundary-tools `prod`, so the machine holds a single Fly secret,
`INFISICAL_TOKEN`, and a rotation in Infisical takes effect on the next
restart. The runner's GitHub identity is `ATB_GITHUB_TOKEN` (or `GH_TOKEN` when set),
already in that project. The agent's Claude Code CLI runs on its own login,
made once on the machine (`fly ssh console -a atb2-runner`, then `claude`)
and kept under HOME on the volume; no Claude credential is stored anywhere
or passed by atb2, on the runner or on a laptop. None of it goes to CI.

By hand, from the repo root:

```sh
fly apps create atb2-runner                                            # once (exists)
fly volumes create atb2_data --size 80 --region sjc -a atb2-runner     # once
fly secrets set -a atb2-runner INFISICAL_TOKEN=...                     # once
fly deploy tools/atb2 --config tools/atb2/deploy/fly.toml
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
