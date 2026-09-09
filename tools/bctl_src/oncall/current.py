"""Find a rotation's current on-call assignee in the local schedule."""

from datetime import date, datetime
from pathlib import Path
from zoneinfo import ZoneInfo

from .parser import parse


def current_oncall(
    today: date | None = None, *, rotation: str = "oncall-releases"
) -> list[str]:
    """Return the rotation's assignee, using today's Pacific date by default."""
    schedule_path = Path(__file__).parent / "data" / "schedule.oncall"
    schedule = parse(schedule_path.read_text())
    if rotation not in schedule.roster.rotations_in_order():
        raise ValueError(f"unknown on-call rotation: {rotation}")
    if today is None:
        today = datetime.now(ZoneInfo("America/Los_Angeles")).date()
    current = max(
        (shift for shift in schedule.shifts if shift.date <= today),
        key=lambda shift: shift.date,
        default=None,
    )
    if current is None:
        raise RuntimeError("no on-call shift covers today")
    name = current.assignments.get(rotation)
    if not name or name not in schedule.roster.by_rotation[rotation]:
        raise RuntimeError(f"no valid current assignee for {rotation}")
    return [name]
