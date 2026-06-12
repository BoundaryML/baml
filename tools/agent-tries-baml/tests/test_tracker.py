"""Unit tests for the cursor-tracker state machine
(``services/notion_fixer/tracker.py``).

Drives ``CursorTracker._track_one`` against an in-memory fake service with the
Cursor + GitHub clients stubbed, covering every transition: tocursor->prprep on a
PR, tocursor->needs_human when the agent gives up, prprep->pr_ready on green,
prprep refix on red (with attempt increment + per-sha dedup + the cap), and
prprep->closed on merge.
"""

from __future__ import annotations

import pytest

from services.notion_fixer import tracker as T

CR = "coderabbitai[bot]"
PR_URL = "https://github.com/o/r/pull/5"


class FakeService:
    """Minimal in-memory stand-in for the ServiceClient verbs the tracker uses."""

    def __init__(self, issue: dict):
        self.issues = {issue["_id"]: dict(issue)}
        self.transitions: list[tuple] = []  # (id, to, patch)
        self.updates: list[tuple] = []      # (id, patch)

    async def list(self, table, **q):
        f, v = q.get("field"), q.get("value")
        return [dict(i) for i in self.issues.values() if i.get(f) == v]

    async def get(self, table, id):
        return dict(self.issues[id]) if id in self.issues else None

    async def update(self, table, id, patch):
        self.issues[id].update(patch)
        self.updates.append((id, dict(patch)))
        return dict(self.issues[id])

    async def transition(self, table, id, to, *, field="status", patch=None, release_claim=True):
        self.issues[id]["status"] = to
        if patch:
            self.issues[id].update(patch)
        self.transitions.append((id, to, dict(patch or {})))
        return dict(self.issues[id])


def _patch_clients(monkeypatch, *, pr_for_agent, get_pr=None, check_runs=None,
                   reviews=None, comments=None, launches=None):
    """Install async stubs for the cursor + github calls the tracker makes."""
    async def _pra(api_key, agent_id, **k):
        return pr_for_agent

    async def _get_pr(o, r, n, **k):
        return get_pr or {}

    async def _checks(o, r, sha, **k):
        return check_runs or []

    async def _reviews(o, r, n, **k):
        return reviews or []

    async def _comments(o, r, n, **k):
        return comments or []

    async def _launch(api_key, prompt, repo_url, ref, **k):
        if launches is not None:
            launches.append({"repo_url": repo_url, "ref": ref, "prompt": prompt})
        return {"id": "stub-agent"}

    monkeypatch.setattr(T.cursor_client, "pr_for_agent", _pra)
    monkeypatch.setattr(T.cursor_client, "launch_agent", _launch)
    monkeypatch.setattr(T.github_client, "get_pr", _get_pr)
    monkeypatch.setattr(T.github_client, "check_runs", _checks)
    monkeypatch.setattr(T.github_client, "pr_reviews", _reviews)
    monkeypatch.setattr(T.github_client, "pr_review_comments", _comments)


def _tracker():
    return T.CursorTracker(None)  # service injected per-call via _track_one's issue arg


# ---------------- tocursor ----------------

async def test_tocursor_to_prprep_when_pr_opens(monkeypatch):
    svc = FakeService({"_id": "i1", "status": "tocursor", "cursorAgentId": "a1", "title": "t"})
    _patch_clients(monkeypatch, pr_for_agent={
        "prUrl": PR_URL, "branch": "b", "runStatus": "RUNNING", "agentStatus": "RUNNING"})
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="tocursor"))[0])
    assert svc.issues["i1"]["status"] == "prprep"
    assert svc.issues["i1"]["prUrl"] == PR_URL
    assert svc.issues["i1"]["prNumber"] == 5


async def test_tocursor_to_needs_human_when_agent_finishes_without_pr(monkeypatch):
    svc = FakeService({"_id": "i1", "status": "tocursor", "cursorAgentId": "a1", "title": "t"})
    _patch_clients(monkeypatch, pr_for_agent={
        "prUrl": None, "branch": None, "runStatus": "FINISHED", "agentStatus": "FINISHED"})
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="tocursor"))[0])
    assert svc.issues["i1"]["status"] == "needs_human"


async def test_tocursor_waits_while_agent_running_without_pr(monkeypatch):
    svc = FakeService({"_id": "i1", "status": "tocursor", "cursorAgentId": "a1", "title": "t"})
    _patch_clients(monkeypatch, pr_for_agent={
        "prUrl": None, "branch": None, "runStatus": "RUNNING", "agentStatus": "RUNNING"})
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="tocursor"))[0])
    assert svc.issues["i1"]["status"] == "tocursor"  # no change


# ---------------- prprep ----------------

def _prprep_issue(**over):
    base = {"_id": "i2", "status": "prprep", "cursorAgentId": "a1", "prUrl": PR_URL,
            "prBranch": "b", "prNumber": 5, "title": "t"}
    base.update(over)
    return base


async def test_prprep_to_pr_ready_on_green(monkeypatch):
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s1"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "success"}],
        reviews=[{"user": {"login": CR}, "state": "COMMENTED"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert svc.issues["i2"]["status"] == "pr_ready"


async def test_prprep_merged_to_closed(monkeypatch):
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": True, "head": {"sha": "s1"}},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert svc.issues["i2"]["status"] == "closed"


async def test_prprep_red_dispatches_refix_and_increments(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s1"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "failure"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert len(launches) == 1
    assert launches[0]["ref"] == "b"  # new agent starts from the PR branch
    assert svc.issues["i2"]["fixAttempts"] == 1
    assert svc.issues["i2"]["lastFixedSha"] == "s1"
    assert svc.issues["i2"]["status"] == "prprep"  # stays, keeps watching
    assert svc.issues["i2"]["cursorAgentId"].startswith("bc-")


async def test_prprep_skips_refix_for_already_fixed_sha(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue(lastFixedSha="s1", fixAttempts=1))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s1"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "failure"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []  # already dispatched a fix for s1
    assert svc.issues["i2"]["fixAttempts"] == 1


async def test_prprep_red_caps_at_max_attempts(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue(fixAttempts=3))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s2"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "failure"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []
    assert svc.issues["i2"]["status"] == "needs_human"


async def test_prprep_waits_while_agent_still_pushing(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "RUNNING",
                      "agentStatus": "RUNNING"},
        get_pr={"merged": False, "head": {"sha": "s1"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "failure"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []  # agent not terminal -> don't pile on a new fix
    assert svc.issues["i2"]["status"] == "prprep"


async def test_prprep_blocked_by_coderabbit_dispatches_refix(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s3"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "success"}],
        reviews=[{"user": {"login": CR}, "state": "CHANGES_REQUESTED",
                  "body": "fix the edge case"}],
        comments=[{"user": {"login": CR}, "path": "x.py", "line": 1, "body": "nit"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert len(launches) == 1  # CodeRabbit blocking triggers a refix even when CI is green
    assert "fix the edge case" in launches[0]["prompt"]
