"""Compose weekly handoff messages.

The handoff is posted the day before a shift starts (Thursday 4pm Pacific for
our Friday shifts) so the incoming oncaller can prepare the release PR ahead of
time. Two follow-up reminders are scheduled via Slack for the shift's first
day, at 9am and 12pm Pacific, walking through the rest of the release
checklist.
"""

from __future__ import annotations

import datetime
from dataclasses import dataclass
from typing import Any, Optional
from zoneinfo import ZoneInfo

from oncall.parser import ScheduleFile, ShiftLine

PACIFIC = ZoneInfo("America/Los_Angeles")

# Local (Pacific) times on the shift's first day at which the scheduled
# reminders are delivered. Order matters: reminder N pairs with checklist
# step N+1 (step 1 is delivered in the handoff itself).
REMINDER_TIMES: tuple[datetime.time, ...] = (
    datetime.time(9, 0),
    datetime.time(12, 0),
)

README_URL = "https://github.com/BoundaryML/baml/tree/canary/tools/bctl_src/oncall/README.md"
RELEASING_URL = "https://github.com/BoundaryML/baml/blob/canary/baml_language/RELEASING.md"
CHANGELOG_URL = "https://github.com/BoundaryML/baml/blob/canary/docs/prepare-changelog.md"


@dataclass(frozen=True)
class HandoffMessage:
    channel: str
    text: str
    blocks: list[dict[str, Any]]
    # None means "post immediately"; otherwise the message is scheduled with
    # Slack's chat.scheduleMessage for delivery at this (tz-aware) instant.
    post_at: Optional[datetime.datetime] = None


def _current_shift(sched: ScheduleFile, today: datetime.date) -> Optional[ShiftLine]:
    shifts = sorted(sched.shifts, key=lambda s: s.date)
    current: Optional[ShiftLine] = None
    for s in shifts:
        if s.date <= today:
            current = s
        else:
            break
    return current


def _handoff_shift(sched: ScheduleFile, today: datetime.date) -> Optional[ShiftLine]:
    """The shift being handed off: the first one starting on or after `today`.

    Run on Thursday, this is Friday's shift. Run on Friday itself (e.g. a
    manual dispatch), it is still that same shift.
    """
    for s in sorted(sched.shifts, key=lambda s: s.date):
        if s.date >= today:
            return s
    return None


def _fmt_date(d: datetime.date) -> str:
    return f"{d.strftime('%a %b')} {d.day}"


def _fmt_when(dt: datetime.datetime) -> str:
    local = dt.astimezone(PACIFIC)
    return f"{_fmt_date(local.date())} {local.strftime('%-I:%M%p').lower()} PT"


def _message(
    channel: str,
    sections: list[str],
    context: list[str],
    post_at: Optional[datetime.datetime] = None,
) -> HandoffMessage:
    blocks: list[dict[str, Any]] = [
        {"type": "section", "text": {"type": "mrkdwn", "text": text}}
        for text in sections
    ]
    blocks.extend(
        {"type": "context", "elements": [{"type": "mrkdwn", "text": text}]}
        for text in context
    )
    body = "\n\n".join(sections + context)
    return HandoffMessage(channel, body, blocks, post_at)


def compose_handoff(
    sched: ScheduleFile,
    now: datetime.datetime,
    wc,
) -> list[HandoffMessage]:
    """Return the handoff announcement plus its scheduled reminders.

    The first message per rotation has `post_at=None` and is meant to go out
    immediately; the rest carry a `post_at` on the shift's first day. Reminder
    times that have already passed as of `now` (e.g. a manual run on Friday
    afternoon) are dropped, and the announcement only lists the reminders that
    will actually be scheduled.

    If `wc` is None, no Slack user-id lookup happens and the @-mention is
    rendered as the bare name (dry-run mode).
    """
    if now.tzinfo is None:
        raise ValueError("now must be timezone-aware")
    today = now.astimezone(PACIFIC).date()
    shift = _handoff_shift(sched, today)
    if shift is None:
        raise RuntimeError("no schedule line starts on or after today")

    sorted_shifts = sorted(sched.shifts, key=lambda s: s.date)
    shift_idx = sorted_shifts.index(shift)
    channel = sched.slack_config.notification_channel

    msgs: list[HandoffMessage] = []
    for rot in sched.roster.rotations_in_order():
        if rot not in shift.assignments:
            raise RuntimeError(
                f"shift {shift.date.isoformat()} has no assignee for rotation {rot!r}"
            )
        incoming_name = shift.assignments[rot]
        if wc is None:
            mention = f"@{incoming_name}"
        else:
            from oncall.slack import email_for, lookup_user_id

            mention = f"<@{lookup_user_id(wc, email_for(incoming_name))}>"

        prev_name: Optional[str] = None
        for s in reversed(sorted_shifts[:shift_idx]):
            if rot in s.assignments:
                prev_name = s.assignments[rot]
                break

        upcoming: list[ShiftLine] = []
        for s in sorted_shifts[shift_idx + 1 :]:
            if rot in s.assignments:
                upcoming.append(s)
                if len(upcoming) >= 3:
                    break

        prev_clause = f" (prev oncall was {prev_name})" if prev_name is not None else ""
        if upcoming:
            upcoming_lines = "\n".join(
                f"- {s.assignments[rot]} goes oncall {_fmt_date(s.date)}"
                for s in upcoming
            )
            upcoming_text = f"Next oncallers:\n{upcoming_lines}"
        else:
            upcoming_text = ""

        footer = f"To swap shifts or update the roster, see <{README_URL}|the oncall README>."

        step_release = (
            "*1. Prep the next release*\n"
            "> Prepare a PR to trigger the next BAML language canary release. See "
            f"<{RELEASING_URL}|baml_language/RELEASING.md> for instructions."
        )
        step_changelog = (
            "*2. Prep the changelog, then review and clean it up before merging it*\n"
            "> Prepare the changelog for the next BAML language canary release: see "
            f"<{CHANGELOG_URL}|docs/prepare-changelog.md>"
        )
        step_thanks = (
            "*3. Tell your agent to thank external contributors.* "
            "`&lt;date&gt;-&lt;version&gt;.todo.md` will have instructions for your agent to handle this for you.\n"
            "> The changelog is published. Find the newest `blog-releases/&lt;date&gt;-&lt;version&gt;.todo.md` "
            "and follow its instructions to thank all external contributors."
        )

        remaining_steps = [step_changelog, step_thanks]
        assert len(remaining_steps) == len(REMINDER_TIMES)
        # (post_at, step number, step text) for reminders still in the future.
        reminders = [
            (at, i + 2, step)
            for i, (t, step) in enumerate(zip(REMINDER_TIMES, remaining_steps))
            if (at := datetime.datetime.combine(shift.date, t, tzinfo=PACIFIC)) > now
        ]

        # Posted immediately (Thursday afternoon): who's up, plus step 1 so the
        # release PR is ready before Friday.
        sections = [
            f"*{rot}* - {mention} is oncall starting {_fmt_date(shift.date)}{prev_clause}",
            step_release,
        ]
        if reminders:
            reminder_schedule = "\n".join(
                f"- {_fmt_when(at)}: reminder for step {n}" for at, n, _ in reminders
            )
            sections.append(
                f"Reminders for the remaining steps are scheduled for:\n{reminder_schedule}"
            )
        msgs.append(
            _message(
                channel,
                sections,
                ([upcoming_text] if upcoming_text else []) + [footer],
            )
        )
        # Scheduled reminders on the shift's first day.
        for at, _, step in reminders:
            msgs.append(
                _message(
                    channel,
                    [f"*{rot}* - {mention} release reminder", step],
                    [footer],
                    post_at=at,
                )
            )
    return msgs
