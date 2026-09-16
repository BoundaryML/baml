"""Tests for oncall parser, validation, and fill_horizon."""

from __future__ import annotations

import datetime
import textwrap
from zoneinfo import ZoneInfo

import pytest

from oncall import cli, slack
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


def test_slack_post_returns_ts_and_schedule_uses_it_as_thread_ts():
    class FakeWebClient:
        def __init__(self):
            self.scheduled = None

        def chat_postMessage(self, **kwargs):
            return {"ts": "123.456"}

        def chat_scheduleMessage(self, **kwargs):
            self.scheduled = kwargs

    wc = FakeWebClient()
    thread_ts = slack.post(wc, "#oncall", "handoff")
    slack.schedule(
        wc,
        "#oncall",
        "reminder",
        datetime.datetime(2026, 4, 24, 9, tzinfo=datetime.timezone.utc),
        thread_ts=thread_ts,
    )

    assert thread_ts == "123.456"
    assert wc.scheduled["thread_ts"] == "123.456"


def test_notify_schedules_reminders_in_handoff_thread(tmp_path, monkeypatch):
    today = datetime.datetime.now(ZoneInfo("America/Los_Angeles")).date()
    schedule_path = tmp_path / "schedule.oncall"
    metadata = textwrap.dedent("""\
        # BEGIN_ONCALL_METADATA
        # slack_config:
        #   notification_channel: "#oncall"
        #
        # roster_by_rotation:
        #   founders: [vbv]
        #   support: [aaron]
        #
        # roster_by_human:
        #   aaron: [support]
        #   vbv: [founders]
        # END_ONCALL_METADATA
    """)
    schedule_path.write_text(
        metadata + f"\n{today.isoformat()} founders=vbv support=aaron\n"
    )

    posted = []
    scheduled = []
    wc = object()

    monkeypatch.setattr(cli, "_schedule_path", lambda: schedule_path)
    monkeypatch.setattr(slack, "client", lambda: wc)
    monkeypatch.setattr(slack, "lookup_user_id", lambda _wc, email: f"U-{email}")

    def fake_post(_wc, channel, text, *, blocks=None):
        posted.append((channel, text, blocks))
        return f"123.{len(posted)}"

    def fake_schedule(
        _wc,
        channel,
        text,
        post_at,
        *,
        blocks=None,
        thread_ts=None,
    ):
        scheduled.append((channel, text, post_at, blocks, thread_ts))

    monkeypatch.setattr(slack, "post", fake_post)
    monkeypatch.setattr(slack, "schedule", fake_schedule)

    cli.notify(post_to_slack=True)

    assert len(posted) == 2
    assert len(scheduled) == 4
    assert [call[4] for call in scheduled] == [
        "123.1",
        "123.1",
        "123.2",
        "123.2",
    ]
