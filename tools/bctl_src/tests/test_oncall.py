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
