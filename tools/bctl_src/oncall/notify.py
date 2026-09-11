"""Thursday release handoff and Friday reminders, with durable send intents."""

from __future__ import annotations

import datetime
import re
from typing import Optional
from zoneinfo import ZoneInfo

from oncall.parser import ScheduleFile, ShiftLine

PACIFIC = ZoneInfo("America/Los_Angeles")


def _current_shift(sched: ScheduleFile, today: datetime.date) -> Optional[ShiftLine]:
    return max((s for s in sched.shifts if s.date <= today), key=lambda s: s.date, default=None)


def notification_friday(reference: datetime.datetime) -> datetime.date:
    """Use the original run's Pacific Thursday, including on later reruns."""
    if reference.tzinfo is None:
        raise RuntimeError("notification reference must include a timezone")
    thursday = reference.astimezone(PACIFIC).date()
    if thursday.weekday() != 3:
        raise RuntimeError("start notifications on Thursday; rerun the original Thursday run to recover")
    return thursday + datetime.timedelta(days=1)


def compose_handoff(sched: ScheduleFile, friday: datetime.date, wc) -> dict:
    """Select the exact incoming Friday shift without changing the current shift."""
    if friday.weekday() != 4:
        raise RuntimeError("the incoming shift must start on Friday")
    shift = next((s for s in sched.shifts if s.date == friday), None)
    name = shift.assignments.get("oncall-releases") if shift else None
    if not name or name not in sched.roster.by_rotation.get("oncall-releases", []):
        raise RuntimeError(f"no valid incoming release oncaller for {friday}")
    if wc is None:
        mention = f"@{name}"
    else:
        from oncall.slack import email_for, lookup_user_id

        user_id = lookup_user_id(wc, email_for(name))
        if not re.fullmatch(r"[UW][A-Z0-9]+", user_id):
            raise RuntimeError("Slack lookup did not return a real user ID")
        mention = f"<@{user_id}>"
    return {
        "version": 1,
        "friday": friday.isoformat(),
        "channel": sched.slack_config.notification_channel,
        "mention": mention,
        "parent": {
            "status": "ready",
            "text": f"Hey {mention}, you’re going to be oncall tomorrow! You’ll be in charge of putting out a canary release tomorrow and monitoring Discord over the weekend and next week. Your rotation runs Friday through Friday.",
        },
        "reminders": [
            {
                "status": "ready",
                "post_at": int(datetime.datetime.combine(friday, datetime.time(hour), PACIFIC).timestamp()),
                "text": f"Hey {mention}, {text}",
            }
            for hour, text in [
                (9, "reminder to put out the canary release today!"),
                (15, "reminder to make sure the canary release has succeeded and the changelog is posted!"),
            ]
        ],
    }


def deliver_handoff(wc, state: dict, save, *, now=None) -> None:
    """Checkpoint before each Slack mutation; never replay an uncertain request.

    `save` must durably compare-and-swap the journal before returning. A stale
    writer must fail, including the first writer creating a week's journal.
    """
    now = now or (lambda: datetime.datetime.now(PACIFIC))
    friday = datetime.date.fromisoformat(state["friday"])
    if state.get("version") != 1:
        raise RuntimeError("unsupported notification state version")
    operations = [state["parent"], *state["reminders"]]
    for operation in operations:
        if operation["status"] not in {"ready", "sent"}:
            raise RuntimeError(
                f"Uncertain Slack send for {friday}; reconcile the pending operation in "
                "oncall/notification-state before retrying (see the oncall README)"
            )
    parent = state["parent"]
    if parent["status"] == "ready":
        local_now = now().astimezone(PACIFIC)
        thursday = friday - datetime.timedelta(days=1)
        if local_now.date() != thursday or local_now.hour < 17:
            raise RuntimeError("a new parent can only be posted Thursday at/after 5pm Pacific")
        parent["status"] = "pending"
        save(state)
        response = wc.chat_postMessage(
            channel=state["channel"], text=parent["text"],
            unfurl_links=False, unfurl_media=False,
        )
        if not response["ok"]:
            raise RuntimeError("Slack rejected the parent; reconcile pending state before retrying")
        # Preserve the actual channel ID and original parent timestamp as strings.
        parent.update(status="sent", channel=response["channel"], ts=response["ts"])
        save(state)
    for reminder in state["reminders"]:
        if reminder["status"] == "sent":
            continue
        if reminder["post_at"] <= now().timestamp():
            raise RuntimeError("Friday reminder deadline passed; reconcile manually, do not send a late duplicate")
        reminder["status"] = "pending"
        save(state)
        # Slack supports thread_ts, but metadata prevents scheduled delivery:
        # https://docs.slack.dev/reference/methods/chat.scheduleMessage/
        response = wc.chat_scheduleMessage(
            channel=parent["channel"], thread_ts=parent["ts"],
            post_at=reminder["post_at"], text=reminder["text"],
            unfurl_links=False, unfurl_media=False,
        )
        if not response["ok"]:
            raise RuntimeError("Slack rejected the reminder; reconcile pending state before retrying")
        reminder.update(status="sent", scheduled_message_id=response["scheduled_message_id"])
        save(state)
