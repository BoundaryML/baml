"""Unit tests for the GitHub PR client's pure decision helpers
(``libs/bench_core/github_client.py``).

Covers PR-url parsing and the CI / CodeRabbit reductions + failure-summary
assembly against canned GitHub JSON — no network.
"""

from __future__ import annotations

from bench_core import github_client as gh

CR = "coderabbitai[bot]"


def _run(name, status, conclusion=None, **extra):
    return {"name": name, "status": status, "conclusion": conclusion, **extra}


def test_parse_pr_url():
    assert gh.parse_pr_url("https://github.com/BoundaryML/baml/pull/123") == (
        "BoundaryML", "baml", 123)
    assert gh.parse_pr_url("not a url") is None


def test_ci_state_empty_is_passing():
    # No checks -> passing (the CodeRabbit gate still holds the PR).
    assert gh.ci_state([]) == "passing"


def test_ci_state_pending_beats_failing():
    runs = [_run("a", "completed", "failure"), _run("b", "in_progress")]
    assert gh.ci_state(runs) == "pending"


def test_ci_state_failing_when_all_complete():
    runs = [_run("a", "completed", "success"), _run("b", "completed", "failure")]
    assert gh.ci_state(runs) == "failing"


def test_ci_state_passing():
    runs = [_run("a", "completed", "success"), _run("b", "completed", "neutral")]
    assert gh.ci_state(runs) == "passing"


def test_coderabbit_blocking_on_changes_requested():
    reviews = [{"user": {"login": CR}, "state": "CHANGES_REQUESTED"}]
    assert gh.coderabbit_state(reviews, []) == "blocking"


def test_coderabbit_blocking_on_failed_check():
    runs = [_run("CodeRabbit", "completed", "failure")]
    assert gh.coderabbit_state([], runs) == "blocking"


def test_coderabbit_clear_when_reviewed_not_blocking():
    reviews = [{"user": {"login": CR}, "state": "COMMENTED"}]
    assert gh.coderabbit_state(reviews, []) == "clear"


def test_coderabbit_none_when_not_reviewed():
    reviews = [{"user": {"login": "somehuman"}, "state": "APPROVED"}]
    assert gh.coderabbit_state(reviews, []) == "none"


def test_coderabbit_latest_review_wins():
    # An earlier CHANGES_REQUESTED resolved by a later COMMENTED -> clear.
    reviews = [
        {"user": {"login": CR}, "state": "CHANGES_REQUESTED"},
        {"user": {"login": CR}, "state": "COMMENTED"},
    ]
    assert gh.coderabbit_state(reviews, []) == "clear"


def test_failure_summary_includes_checks_and_coderabbit():
    runs = [_run("unit-tests", "completed", "failure",
                 output={"summary": "3 tests failed"}, details_url="http://ci/1")]
    reviews = [{"user": {"login": CR}, "state": "CHANGES_REQUESTED",
                "body": "Please handle the None case."}]
    comments = [{"user": {"login": CR}, "path": "src/x.py", "line": 10,
                 "body": "unused import"}]
    out = gh.failure_summary(runs, reviews, comments)
    assert "unit-tests" in out and "3 tests failed" in out
    assert "Please handle the None case." in out
    assert "src/x.py" in out and "unused import" in out


def test_human_comment_summary_includes_humans_skips_bots():
    review_comments = [
        {"user": {"type": "User", "login": "alice"}, "path": "a.rs", "line": 4, "body": "rename this"},
        {"user": {"type": "Bot", "login": CR}, "path": "a.rs", "line": 9, "body": "nit"},
    ]
    issue_comments = [
        {"user": {"type": "User", "login": "bob"}, "body": "add a test for the empty case"},
        {"user": {"type": "Bot", "login": "cursor[bot]"}, "body": "opened a PR"},
    ]
    out = gh.human_comment_summary(review_comments, issue_comments)
    assert "@alice" in out and "rename this" in out
    assert "@bob" in out and "add a test for the empty case" in out
    assert "nit" not in out and "opened a PR" not in out  # bot comments excluded
