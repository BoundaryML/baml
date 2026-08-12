"""Validate, canonicalize, and extend the oncall schedule."""

from __future__ import annotations

import datetime
import re
from dataclasses import dataclass
from typing import Optional

from oncall.parser import Roster, ScheduleFile, ShiftLine, emit


_CHANNEL_NAME_RE = re.compile(r"^#[a-z0-9_-]+$")
_CHANNEL_ID_RE = re.compile(r"^C[A-Z0-9]+$")


@dataclass
class ValidationError:
    line: Optional[int]
    message: str
    fixable: bool


def canonicalize(sched: ScheduleFile) -> ScheduleFile:
    rotations_order = sched.roster.rotations_in_order()
    new_shifts: list[ShiftLine] = []
    for shift in sched.shifts:
        reordered: dict[str, str] = {}
        for rot in rotations_order:
            if rot in shift.assignments:
                reordered[rot] = shift.assignments[rot]
        for rot, val in shift.assignments.items():
            if rot not in reordered:
                reordered[rot] = val
        new_shifts.append(
            ShiftLine(
                date=shift.date,
                assignments=reordered,
                trailing_comment=shift.trailing_comment,
            )
        )
    return ScheduleFile(
        slack_config=sched.slack_config,
        roster=sched.roster,
        shifts=new_shifts,
        free_comments=list(sched.free_comments),
        declared_by_human=sched.roster.by_human(),
    )


def validate(sched: ScheduleFile, raw_text: str) -> list[ValidationError]:
    errors: list[ValidationError] = []

    ch = sched.slack_config.notification_channel
    if not (_CHANNEL_NAME_RE.match(ch) or _CHANNEL_ID_RE.match(ch)):
        errors.append(
            ValidationError(
                line=None,
                message=f"slack_config.notification_channel {ch!r} must match #name or channel-id",
                fixable=False,
            )
        )

    computed = sched.roster.by_human()
    declared = sched.declared_by_human
    if declared is None or declared != computed:
        errors.append(
            ValidationError(
                line=None,
                message="roster_by_human is missing or does not match roster_by_rotation",
                fixable=True,
            )
        )

    seen_dates: set[datetime.date] = set()
    prev_date: Optional[datetime.date] = None
    rotations_order = sched.roster.rotations_in_order()
    for shift in sched.shifts:
        date_s = shift.date.isoformat()
        if shift.date in seen_dates:
            errors.append(
                ValidationError(
                    line=None,
                    message=f"duplicate shift date {date_s}",
                    fixable=False,
                )
            )
        if prev_date is not None and shift.date < prev_date:
            errors.append(
                ValidationError(
                    line=None,
                    message=f"shift {date_s} is out of chronological order (follows {prev_date.isoformat()})",
                    fixable=False,
                )
            )
        seen_dates.add(shift.date)
        prev_date = shift.date

        if "none" in shift.assignments:
            errors.append(
                ValidationError(
                    line=None,
                    message=f"{date_s}: 'none=' is not allowed on shift lines",
                    fixable=False,
                )
            )

        for rot, assignee in shift.assignments.items():
            if rot == "none":
                continue
            if rot not in sched.roster.by_rotation:
                errors.append(
                    ValidationError(
                        line=None,
                        message=f"{date_s}: unknown rotation {rot!r}",
                        fixable=False,
                    )
                )
                continue
            if assignee not in sched.roster.by_rotation[rot]:
                errors.append(
                    ValidationError(
                        line=None,
                        message=f"{date_s}: {assignee!r} is not in rotation {rot!r}",
                        fixable=False,
                    )
                )

        shift_order = [r for r in shift.assignments.keys() if r != "none"]
        expected_order = [r for r in rotations_order if r in shift.assignments]
        if shift_order != expected_order:
            errors.append(
                ValidationError(
                    line=None,
                    message=f"{date_s}: k=v pair order must match roster_by_rotation declaration order",
                    fixable=True,
                )
            )

    try:
        canonical_text = emit(canonicalize(sched))
    except Exception:
        canonical_text = None
    if canonical_text is not None and canonical_text != raw_text:
        if not any(e.fixable for e in errors):
            errors.append(
                ValidationError(
                    line=None,
                    message="file is not in canonical form (whitespace/formatting)",
                    fixable=True,
                )
            )

    return errors


def fill_horizon(
    sched: ScheduleFile,
    today: datetime.date,
    target_months: int = 4,
    trigger_months: int = 1,
) -> ScheduleFile:
    for prev, curr in zip(sched.shifts, sched.shifts[1:]):
        assert prev.date <= curr.date, (
            f"shifts not chronologically ordered: {prev.date} precedes {curr.date}"
        )

    target_days = target_months * 30
    trigger_days = trigger_months * 30

    if sched.shifts:
        horizon_end = max(s.date for s in sched.shifts)
    else:
        horizon_end = today

    if (horizon_end - today).days >= trigger_days:
        return sched

    if sched.shifts:
        next_date = horizon_end + datetime.timedelta(days=7)
    else:
        days_until_friday = (4 - today.weekday()) % 7
        next_date = today + datetime.timedelta(days=days_until_friday)

    target_end = today + datetime.timedelta(days=target_days)
    rotations = sched.roster.rotations_in_order()

    next_idx: dict[str, int] = {}
    for rot in rotations:
        roster = sched.roster.by_rotation[rot]
        if not roster:
            raise ValueError(f"rotation {rot!r} has an empty roster")
        last_assignee: Optional[str] = None
        for s in reversed(sched.shifts):
            if rot in s.assignments:
                last_assignee = s.assignments[rot]
                break
        if last_assignee is None or last_assignee not in roster:
            next_idx[rot] = 0
        else:
            next_idx[rot] = (roster.index(last_assignee) + 1) % len(roster)

    existing_dates = {s.date for s in sched.shifts}
    new_shifts = list(sched.shifts)

    current = next_date
    while current <= target_end:
        if current not in existing_dates:
            assignments: dict[str, str] = {}
            for rot in rotations:
                roster = sched.roster.by_rotation[rot]
                assignments[rot] = roster[next_idx[rot]]
                next_idx[rot] = (next_idx[rot] + 1) % len(roster)
            new_shifts.append(ShiftLine(date=current, assignments=assignments))
        current = current + datetime.timedelta(days=7)

    new_shifts.sort(key=lambda s: s.date)
    return ScheduleFile(
        slack_config=sched.slack_config,
        roster=sched.roster,
        shifts=new_shifts,
        free_comments=list(sched.free_comments),
        declared_by_human=sched.declared_by_human,
    )
