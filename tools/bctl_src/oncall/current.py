"""Find the current release on-call assignee in the local schedule."""

from datetime import date, datetime
from pathlib import Path
from zoneinfo import ZoneInfo

from .parser import parse


def current_oncall(today: date | None = None) -> list[str]:
    """Return the release assignee, using today's Pacific date by default."""
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
    name = current.assignments.get("oncall-releases")
    if not name or name not in schedule.roster.by_rotation.get("oncall-releases", []):
        raise RuntimeError("no valid current assignee for oncall-releases")
    return [name]
