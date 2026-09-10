"""Compose weekly handoff messages."""

from __future__ import annotations

import datetime
from dataclasses import dataclass
from typing import Any, Optional

from oncall.parser import ScheduleFile, ShiftLine


@dataclass(frozen=True)
class HandoffMessage:
    channel: str
    text: str
    blocks: list[dict[str, Any]]


def _current_shift(sched: ScheduleFile, today: datetime.date) -> Optional[ShiftLine]:
    shifts = sorted(sched.shifts, key=lambda s: s.date)
    current: Optional[ShiftLine] = None
    for s in shifts:
        if s.date <= today:
            current = s
        else:
            break
    return current


def _fmt_date(d: datetime.date) -> str:
    return f"{d.strftime('%a %b')} {d.day}"


def compose_handoff(
    sched: ScheduleFile,
    today: datetime.date,
    wc,
) -> list[HandoffMessage]:
    """Return Slack blocks and fallback text for the weekly handoff.

    If `wc` is None, no Slack user-id lookup happens and the @-mention is
    rendered as the bare name (dry-run mode).
    """
    current = _current_shift(sched, today)
    if current is None:
        raise RuntimeError("no schedule line covers the current week")

    sorted_shifts = sorted(sched.shifts, key=lambda s: s.date)
    current_idx = sorted_shifts.index(current)

    msgs: list[HandoffMessage] = []
    for rot in sched.roster.rotations_in_order():
        if rot not in current.assignments:
            raise RuntimeError(
                f"current week {current.date.isoformat()} has no assignee for rotation {rot!r}"
            )
        incoming_name = current.assignments[rot]
        if wc is None:
            mention = f"@{incoming_name}"
        else:
            from oncall.slack import email_for, lookup_user_id

            mention = f"<@{lookup_user_id(wc, email_for(incoming_name))}>"

        prev_name: Optional[str] = None
        for s in reversed(sorted_shifts[:current_idx]):
            if rot in s.assignments:
                prev_name = s.assignments[rot]
                break

        upcoming: list[ShiftLine] = []
        for s in sorted_shifts[current_idx + 1 :]:
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

        footer = "To swap shifts or update the roster, see <https://github.com/BoundaryML/baml/tree/canary/tools/bctl_src/oncall/README.md|the oncall README>."

        sections = [
            f"*{rot}* - {mention} is oncall starting {_fmt_date(current.date)}{prev_clause}",
            "*1. Prep the next release*\n"
            "> Prepare a PR to trigger the next BAML language canary release. See "
            "<https://github.com/BoundaryML/baml/blob/canary/"
            "baml_language/RELEASING.md|baml_language/RELEASING.md> for instructions.",
            "*2. Prep the changelog, then review and clean it up before merging it*\n"
            "> Prepare the changelog for the next BAML language canary release: see "
            "<https://github.com/BoundaryML/baml/blob/canary/"
            "docs/prepare-changelog.md|docs/prepare-changelog.md>",
            "*3. Tell your agent to thank external contributors.* "
            "`&lt;version&gt;.todo.md` will have instructions for your agent to handle this for you.\n"
            "> The changelog is published. Find the newest `blog-release/&lt;version&gt;.todo.md` "
            "and follow its instructions to thank all external contributors.",
        ]
        blocks: list[dict[str, Any]] = [
            {"type": "section", "text": {"type": "mrkdwn", "text": text}}
            for text in sections
        ]
        context = ([upcoming_text] if upcoming_text else []) + [footer]
        blocks.extend(
            {"type": "context", "elements": [{"type": "mrkdwn", "text": text}]}
            for text in context
        )
        body = "\n\n".join(sections + context)

        msgs.append(
            HandoffMessage(sched.slack_config.notification_channel, body, blocks)
        )
    return msgs
