"""Compose weekly handoff messages."""

from __future__ import annotations

import datetime
from typing import Optional

from oncall.parser import ScheduleFile, ShiftLine


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
) -> list[tuple[str, str]]:
    """Returns [(channel, text), ...] — one entry per rotation.

    If `wc` is None, no Slack user-id lookup happens and the @-mention is
    rendered as the bare name (dry-run mode).
    """
    current = _current_shift(sched, today)
    if current is None:
        raise RuntimeError("no schedule line covers the current week")

    sorted_shifts = sorted(sched.shifts, key=lambda s: s.date)
    current_idx = sorted_shifts.index(current)

    msgs: list[tuple[str, str]] = []
    for rot in sched.roster.rotations_in_order():
        if rot not in current.assignments:
            raise RuntimeError(f"current week {current.date.isoformat()} has no assignee for rotation {rot!r}")
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
        for s in sorted_shifts[current_idx + 1:]:
            if rot in s.assignments:
                upcoming.append(s)
                if len(upcoming) >= 3:
                    break

        prev_clause = f" (prev oncall was {prev_name})" if prev_name is not None else ""
        if upcoming:
            upcoming_lines = "\n".join(
                f"- {s.assignments[rot]} goes oncall {_fmt_date(s.date)}" for s in upcoming
            )
            upcoming_block = f"\n\nNext oncallers:\n{upcoming_lines}"
        else:
            upcoming_block = ""

        footer = "\n\n_To swap shifts or update the roster, see <https://github.com/BoundaryML/baml/tree/canary/tools/bctl_src/oncall/README.md|the oncall README>._"

        body = f"""*{rot}* - {mention} is oncall starting {_fmt_date(current.date)}{prev_clause}{upcoming_block}{footer}"""

        msgs.append((sched.slack_config.notification_channel, body))
    return msgs
