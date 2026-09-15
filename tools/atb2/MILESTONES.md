# atb2 milestones

This is a living implementation and measurement document. Review counts are goals,
not claims that adoption or quality targets have already been reached.

## Issue MVP

Feedback from the CLI, GitHub and Slack becomes a concrete repro. Production
triage resolves current canary and nightly artifacts through the isolated lazy
cache and records each verdict. A failed check is not a successful confirmation.
Local/eval runs test the selected compiler without claiming live channel coverage.
Similar reports link to an existing issue and add distinct repros. Larger issues
produce an investigation plan; Easy/Trivial issues may produce a draft fix PR.
The small-fix target is about 200 changed lines, an estimate rather than a claim
that line count proves correctness. The shepherd owns the final change and merge.

Slack: `@shepherd title <issue link> · here's my fix: <draft PR link>`.
The first announcement can precede the draft and is updated when it is available.
The issue page contains concise descriptions, repros, version information,
copyable investigation instructions, a timestamped lifecycle and live transcripts.
Configured shepherds can X-cancel an issue. No approvals or automatic merges.

### Five team members merge related PRs

Goal: five distinct team members merge PRs linked to bammy-reported issues.
Record the issue URL, merged PR URL, member and merge timestamp. A draft generated
by bammy is optional: a human-authored related fix counts. Do not count bot pushes.

### Eighteen of twenty are useful

Goal: at least 18 of the next 20 consecutively reviewed bammy issue/fix packages
are judged real and useful by a shepherd. Record the reviewer and explanation,
including rejected, cancelled and duplicate cases. Do not cherry-pick the sample
or treat passing syntax checks as proof of issue quality.

## PR monitor

Depends on the issue MVP's runtime and shared execution primitives. Both an issue
fix PR and `@bammy babysit <PR>` use the same automatic review loop. The agent
reads feedback for the current head, fixes actionable CI/reviewer problems, scans
and pushes, then waits for results on the new head. Slack reports progress in the
same thread. The default budget is seven rounds. A human still merges.

Workflow-permission-only blockers can produce **Green, with workflow changes
needed**, with evidence. This is a qualified completion, not proof that all
required GitHub checks passed. Actual code/test/security/reviewer failures block.
The feature remains experimental until the acceptance sample below succeeds.

### Ten successful uses

Goal: ten completed monitor runs across generated and independently requested
PRs. Record request ID, PR URL, starting/final head, rounds, elapsed time, CI and
review evidence, and any operator intervention. Report qualified workflow-only
completions separately; do not silently count them as fully green runs.

## Agent experience

A linked issue's status and verified fixing release reach `baml feedback
status/list/view`. Ordinary interactive commands check periodically with a bounded
network budget and cached fallback. A known fixing release produces an update
notice; upgrading to a containing release clears it. A merge alone is not proof
that a fixing release is available. Agents get `baml feedback` guidance in the
BAML skill. No numeric adoption target was specified for this milestone.

## Deployment and acceptance

Source lives in `tools/atb2`; the website lives in `typescript2/app-feedback`.
The runner workflow deploys canary changes to the existing `atb2-runner` Fly app
when its token is configured. No production deployment is performed by creating
these PRs. The demo is currently paused. Fly organization ownership and Vercel
production-branch linkage must be checked against their current configurations;
a repository workflow comment is not evidence that either is configured.

Website deployment targets only `app-feedback`. Authenticated comments and raw
transcripts require the GitHub OAuth and signed runner bridge configuration.
Schema changes are applied separately from PR descriptions, never checked in as
DDL. Existing rows remain intact. Live/eval data stays separated.

Per-layer checks: BAML check/fmt/token-free tests, relevant controller regression
tests, website type/lint/build checks for UI changes, and secret/pattern scans of
outgoing commits. Linux namespace isolation requires the container tests; skips
on a developer machine do not count as passing isolation checks. Live smoke tests
and the quantitative samples remain rollout gates, not fabricated test results.

Shirts and scratch-task runs are separate work, preserved on the old branches.
