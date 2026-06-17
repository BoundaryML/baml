"""Linear card rendering: dashboard issue link + PR link in the Markdown body,
the issue-link helper points at the new.boundaryml.com/atb dashboard, and the
Convex-status -> Linear status-label mapping (incl. the redraft self-map)."""

from __future__ import annotations

from bench_core import linear_client as lc
from services.linear_sync import fixer
from services.linear_sync.__main__ import LinearPush


def test_issue_link_uses_atb_dashboard():
    assert fixer.issue_link({"_id": "iss_123"}) == "https://new.boundaryml.com/atb/issues/iss_123"


def test_issue_link_empty_without_id():
    assert fixer.issue_link({}) == ""


def test_evidence_links_use_atb_dashboard():
    issue = {"evidence": [{"trophyId": "t1"}, {"trophyId": "t2", "call_index": 3}]}
    links = fixer.evidence_links(issue)
    assert links == [
        "https://new.boundaryml.com/atb/runs/t1",
        "https://new.boundaryml.com/atb/runs/t2?call=3",
    ]


def test_body_renders_issue_and_pr_links():
    body = lc.issue_body_md(
        "A bug.", [], suggestion=None, category=None, repro=None,
        issue_link="https://new.boundaryml.com/atb/issues/iss_9",
        pr_url="https://github.com/BoundaryML/baml/pull/42",
    )
    assert "**Links:**" in body
    assert "https://new.boundaryml.com/atb/issues/iss_9" in body
    assert "https://github.com/BoundaryML/baml/pull/42" in body


def test_body_omits_links_section_when_absent():
    body = lc.issue_body_md("A bug.", [], suggestion=None, category=None, repro=None)
    assert "Links:" not in body


def test_cursor_prompt_requires_docstrings():
    prompt = fixer.cursor_prompt(
        {"title": "Add modulo", "category": "bug", "kind": "language", "description": "x"})
    low = prompt.lower()
    assert "docstring" in low
    assert "code standards" in low


def test_map_status_covers_the_lifecycle():
    """Each Convex status maps to its Linear status-group label id (default not-started)."""
    assert LinearPush._map_status("open") == lc.LINEAR_STATUS_NOT_STARTED
    assert LinearPush._map_status("confirmed") == lc.LINEAR_STATUS_NOT_STARTED
    assert LinearPush._map_status("approved") == lc.LINEAR_STATUS_APPROVED
    assert LinearPush._map_status("dispatching") == lc.LINEAR_STATUS_APPROVED
    assert LinearPush._map_status("tocursor") == lc.LINEAR_STATUS_TO_CURSOR
    assert LinearPush._map_status("prprep") == lc.LINEAR_STATUS_PR_PREP
    assert LinearPush._map_status("pr_ready") == lc.LINEAR_STATUS_READY_TO_MERGE
    assert LinearPush._map_status("needs_human") == lc.LINEAR_STATUS_NEEDS_HUMAN
    assert LinearPush._map_status("closed") == lc.LINEAR_STATUS_MERGED
    # redraft self-maps so a mid-redraft re-render doesn't clobber the human label
    assert LinearPush._map_status("redraft") == lc.LINEAR_STATUS_REDRAFT
    # unknown -> not-started
    assert LinearPush._map_status("???") == lc.LINEAR_STATUS_NOT_STARTED
