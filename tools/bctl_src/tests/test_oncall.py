"""Tests for oncall parser, validation, and fill_horizon."""

from __future__ import annotations

import datetime
import textwrap

import pytest

from oncall.parser import parse
from oncall.schedule import fill_horizon, validate


METADATA = textwrap.dedent("""\
    # BEGIN_ONCALL_METADATA
    # slack_config:
    #   notification_channel: "#oncall"
    #
    # roster_by_rotation:
    #   founders: [vbv, aaron]
    #   none: [greg]
    #
    # roster_by_human:
    #   aaron: [founders]
    #   greg: [none]
    #   vbv: [founders]
    # END_ONCALL_METADATA
""")


def _blocking(sched, text):
    return [e for e in validate(sched, text) if not e.fixable]


def test_check_flags_empty_assignee():
    text = METADATA + "\n2026-04-24 founders=\n"
    sched = parse(text)
    assert sched.shifts[0].assignments == {"founders": ""}
    blocking = _blocking(sched, text)
    assert any(
        "'' is not in rotation 'founders'" in e.message for e in blocking
    ), [e.message for e in blocking]


def test_check_flags_out_of_order_shifts():
    text = METADATA + "\n2026-04-24 founders=aaron\n2026-04-17 founders=vbv\n"
    sched = parse(text)
    blocking = _blocking(sched, text)
    assert any("chronological order" in e.message for e in blocking), [
        e.message for e in blocking
    ]


def test_fill_horizon_asserts_chronological_order():
    text = METADATA + "\n2026-04-24 founders=aaron\n2026-04-17 founders=vbv\n"
    sched = parse(text)
    with pytest.raises(AssertionError, match="chronologically ordered"):
        fill_horizon(sched, datetime.date(2026, 4, 24))


def test_fill_horizon_extends_when_horizon_short():
    text = METADATA + "\n2026-04-24 founders=aaron\n"
    sched = parse(text)
    filled = fill_horizon(sched, datetime.date(2026, 4, 24))
    assert len(filled.shifts) > len(sched.shifts)
    for prev, curr in zip(filled.shifts, filled.shifts[1:]):
        assert prev.date < curr.date


SCHEDULE_3WK = METADATA + textwrap.dedent("""\

    2026-09-11 founders=vbv none=greg
    2026-09-18 founders=aaron none=greg
    2026-09-25 founders=vbv none=greg
""")


def _handoff(today):
    from oncall.notify import compose_handoff

    return compose_handoff(parse(SCHEDULE_3WK), today, None)


def test_handoff_on_thursday_targets_fridays_shift():
    msgs = _handoff(datetime.date(2026, 9, 17))  # Thursday
    founders = [m for m in msgs if m.text.startswith("*founders*")]
    assert founders[0].post_at is None
    assert "@aaron is oncall starting Fri Sep 18" in founders[0].text
    assert "(prev oncall was vbv)" in founders[0].text
    assert "1. Prep the next release" in founders[0].text
    assert "vbv goes oncall Fri Sep 25" in founders[0].text


def test_handoff_on_friday_targets_same_shift():
    msgs = _handoff(datetime.date(2026, 9, 18))  # Friday, manual dispatch
    founders = [m for m in msgs if m.text.startswith("*founders*")]
    assert "@aaron is oncall starting Fri Sep 18" in founders[0].text


def test_handoff_schedules_friday_9am_and_noon_pacific_reminders():
    from zoneinfo import ZoneInfo

    msgs = _handoff(datetime.date(2026, 9, 17))
    founders = [m for m in msgs if m.text.startswith("*founders*")]
    assert len(founders) == 3
    reminders = founders[1:]
    pacific = ZoneInfo("America/Los_Angeles")
    assert [r.post_at for r in reminders] == [
        datetime.datetime(2026, 9, 18, 9, 0, tzinfo=pacific),
        datetime.datetime(2026, 9, 18, 12, 0, tzinfo=pacific),
    ]
    # PDT in September: 9am PT == 16:00 UTC.
    assert reminders[0].post_at.astimezone(datetime.timezone.utc).hour == 16
    assert "2. Prep the changelog" in reminders[0].text
    assert "3. Tell your agent to thank external contributors" in reminders[1].text
    for r in reminders:
        assert "@aaron" in r.text
        assert r.channel == "#oncall"


def test_handoff_announcement_lists_reminder_times():
    msgs = _handoff(datetime.date(2026, 9, 17))
    text = msgs[0].text
    assert "Fri Sep 18 9:00am PT: reminder for step 2" in text
    assert "Fri Sep 18 12:00pm PT: reminder for step 3" in text


def test_handoff_errors_when_no_upcoming_shift():
    import pytest as _pytest

    with _pytest.raises(RuntimeError, match="on or after today"):
        _handoff(datetime.date(2026, 9, 26))
