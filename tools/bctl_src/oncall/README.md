# bctl oncall

`bctl oncall` manages our on-call roster. The source of truth is a single file, [`data/schedule.oncall`](./data/schedule.oncall), that holds both the roster (who is in which rotation) and the week-by-week shift assignments.

# Updating `schedule.oncall`

- to swap shifts with someone, edit `schedule.oncall` - [click here to edit in github.dev](https://github.dev/BoundaryML/baml/blob/canary/tools/bctl_src/oncall/data/schedule.oncall)
- to add someone to the oncall roster, add them to `roster_by_rotation` then run `bctl oncall check --fix` 

# `schedule.oncall` file format

The file has three types of data:

- oncall metadata, defined in YAML comments between `BEGIN_ONCALL_METADATA` and `END_ONCALL_METADATA`
- the schedule itself: `YYYY-MM-DD <rotation>=<person> <rotation>=<person> ...`
- `#` indicates comments

# Tooling

- `bctl oncall current` prints the current `oncall-releases` assignee from the local schedule using today's Pacific date.
- `bctl oncall check [--fix]` parses `schedule.oncall` and validates it, ensuring that the file is well-formed
	- runs on every change to `schedule.oncall`
- `bctl oncall notify [--post-to-slack] [--run-started-at <ISO-8601 timestamp>]`
	- on Thursday at 5pm America/Los_Angeles, notify the exact next Friday's `oncall-releases` assignee in the configured `notification_channel` (currently `#general`)
	- schedule two replies to that original parent thread for Friday at 9am and 3pm Pacific; all three messages mention the same resolved Slack user
	- keep shifts Friday through Friday; the Thursday notice does not change who `bctl oncall current` reports, the roster, or any shift dates
	- without `--post-to-slack`, preview the text and local delivery times without any Slack or GitHub calls; names appear as placeholders only in previews
- `bctl oncall fill-schedule`
	- append weekly shifts to `schedule.oncall`, assigning in a round-robin pattern

# Weekly notification thread

Thursday at 5pm Pacific:

> Hey <actual oncall mention>, you’re going to be oncall tomorrow! You’ll be in charge of putting out a canary release tomorrow and monitoring Discord over the weekend and next week. Your rotation runs Friday through Friday.

Friday at 9am Pacific, in the same thread:

> Hey <same oncall mention>, reminder to put out the canary release today!

Friday at 3pm Pacific, in the same thread:

> Hey <same oncall mention>, reminder to make sure the canary release has succeeded and the changelog is posted!

The [oncall workflow](../../../.github/workflows/oncall.yml) uses a timezone-aware Thursday cron. Python constructs Friday's 9am and 3pm timestamps with `ZoneInfo("America/Los_Angeles")`, so both PST and PDT work. GitHub cron can be delayed; the parent posts when the Thursday job executes, and Slack owns delivery of the queued Friday replies. A new parent is refused outside Thursday at/after 5pm Pacific, and an unscheduled reminder whose deadline has passed fails visibly instead of posting late.

[Slack's `chat.scheduleMessage` reference](https://docs.slack.dev/reference/methods/chat.scheduleMessage/) explicitly supports `thread_ts` and requires the original parent's timestamp. It also warns that scheduled messages using `metadata` will not post. We pass the parent's exact timestamp string and returned channel ID and omit metadata. This API contract is checked with mocked requests in tests, and the manual sandbox action below exercises the same Python scheduling path against Slack. A successful schedule API response confirms queuing, not eventual delivery, so check the thread if Slack delivery is disrupted or someone deletes a queued message.

# Credentials and retry recovery

The Slack client continues to use `SLACK_BOUNDARY_BOT_TOKEN` from the existing `boundary-tools-prod` environment. Production mentions resolve `<name>@boundaryml.com` through `users.lookupByEmail`; lookup failure stops the notification. Existing `users:read.email` and `chat:write` permissions are used; no history-reading scope is needed for normal operation.

The notify job uses its `GITHUB_TOKEN` as `GH_TOKEN` with `contents: write` to maintain `notifications/YYYY-MM-DD.json` on a separate `oncall/notification-state` branch, created automatically from the default branch on first use. `actions: read` retrieves the original workflow run's `created_at`, which remains the date anchor on later reruns. This journal contains message text, the resolved mention, channel ID, parent timestamp, scheduled-message IDs, and operation statuses; it contains no credentials and never modifies `schedule.oncall`. Do not delete or reset the state branch: it is the duplicate-prevention record. Local production invocations require `GITHUB_REPOSITORY=BoundaryML/baml` and authenticated `gh` access to that same journal.

Each Slack mutation is preceded by a durable `pending` checkpoint and followed by a `sent` checkpoint containing its result. GitHub file SHA checks reject concurrent stale writers; workflow concurrency also serializes notify runs. Slack transport retries are disabled for this path because a lost response may still mean the message was accepted. Completed operations are skipped even after Friday delivery, and remaining replies always retain the original Thursday mention and parent timestamp, including if the roster or assignment is subsequently edited.

A `pending` operation indicates an uncertain send (including a killed runner or a lost checkpoint response). The job fails and the existing `notify-failure` job alerts the configured channel. Automatic recovery cannot safely distinguish an accepted request from a lost request; it deliberately refuses to send that operation again. To recover:

1. Stop any active notify run and inspect that Friday's journal on `oncall/notification-state` and the original Slack channel/thread. For queued replies, use [`chat.scheduledMessages.list`](https://docs.slack.dev/reference/methods/chat.scheduledMessages.list/) with the same bot token, following all pagination cursors. The listing exposes IDs, channel, text, and delivery times, but does not document `thread_ts`; do not infer an unknown thread from matching text alone.
2. If Slack accepted the operation, preserve its result in the journal and mark it `sent`: the parent needs its exact string `ts` and `channel` ID; a reminder needs its `scheduled_message_id`. If a reply has already delivered, verify the original thread before marking it sent. If you can establish the request was never accepted, reset only that operation to `ready`. When acceptance cannot be established, leave it pending and investigate; do not blindly reset or delete the journal. Never change the stored mention to a new assignee during recovery.
3. Rerun the original Thursday workflow run. It resumes ready operations whose deadlines are still future, without reposting completed ones. A new Friday dispatch is rejected because it has no Thursday date anchor. If a deadline has passed, handle the missed reminder manually in the original thread and reconcile the journal before rerunning.

For a network-free preview from any day, supply a Thursday timestamp, for example `./tools/bctl oncall notify --run-started-at 2026-09-18T00:00:00Z` (Thursday 5pm PDT). Run the focused suite with `uv run --directory tools --frozen pytest`.

# Live sandbox test through Python

Manually dispatch the existing oncall workflow with action `test-notify` and `sandbox_parent_ts` set to an existing thread in `#sam-sandbox` (`C07UTQN7N1X`). This runs `bctl oncall test-notify` using the existing bot secret in GitHub Actions. It schedules two labeled test replies through the same Python delivery function at approximately 90 and 180 seconds after startup, using the next Friday assignee's real Slack mention. It reuses the supplied parent and creates no new parent post. Production rotation dates and notification journals are unaffected.

Tests are pinned to the sandbox channel and have separate `notifications/sandbox/<run-id>.json` journals. Rerun the same workflow run to reuse its journal and avoid duplicates; a new dispatch intentionally creates a new pair of test messages. The same pending-operation recovery rules apply. Check the actual sandbox thread after the accelerated delivery times to verify Slack delivered both messages as replies; scheduled-message IDs in the workflow log confirm queuing only. Sandbox-test failures are visible in their workflow job and do not trigger the production-channel failure alert.
