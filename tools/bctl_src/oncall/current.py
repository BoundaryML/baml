"""Find the current primary on-call assignees in the local schedule."""

from datetime import date, datetime
from pathlib import Path
from zoneinfo import ZoneInfo

from .parser import parse


def current_oncall(today: date | None = None) -> list[str]:
    """Return primary assignee names, using today's Pacific date by default."""
    schedule_path = Path(__file__).parent / "data" / "schedule.oncall"
    schedule = parse(schedule_path.read_text())
    if today is None:
        today = datetime.now(ZoneInfo("America/Los_Angeles")).date()
    current = max(
        (shift for shift in schedule.shifts if shift.date <= today),
        key=lambda shift: shift.date,
        default=None,
    )
    if current is None:
        raise RuntimeError("no on-call shift covers today")
    # Founders are an escalation rotation, not the primary on-call.
    names = list(
        dict.fromkeys(
            current.assignments[rotation]
            for rotation in schedule.roster.rotations_in_order()
            if rotation != "oncall-founders"
        )
    )
    if not names:
        raise RuntimeError("no primary on-call assignee")
    return names
