"""Unit tests for the cursor-tracker state machine
(``services/linear_sync/tracker.py``).

Drives ``CursorTracker._track_one`` against an in-memory fake service with the
Cursor + GitHub clients stubbed, covering every transition: tocursor->prprep on a
PR, tocursor->needs_human when the agent gives up, prprep->pr_ready on green,
prprep refix on red (with attempt increment + per-sha dedup + the cap), and
prprep->closed on merge.
"""

from __future__ import annotations

import pytest

from services.linear_sync import tracker as T

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
                   reviews=None, comments=None, launches=None, pr_by_branch=None,
                   issue_comments=None, new_comment_ids=None, reactions=None):
    """Install async stubs for the cursor + github calls the tracker makes.

    ``new_comment_ids`` is the set of comment ids ``add_reaction`` reports as newly
    reacted (201 / unseen); ``reactions`` (if given) records each ``(kind, id)`` call.
    """
    fresh = set(new_comment_ids or ())

    async def _pra(api_key, agent_id, **k):
        return pr_for_agent

    async def _open_pr(owner, repo, branch, **k):
        return pr_by_branch

    async def _get_pr(o, r, n, **k):
        return get_pr or {}

    async def _checks(o, r, sha, **k):
        return check_runs or []

    async def _reviews(o, r, n, **k):
        return reviews or []

    async def _comments(o, r, n, **k):
        return comments or []

    async def _issue_comments(o, r, n, **k):
        return issue_comments or []

    async def _add_reaction(o, r, comment_id, *, kind="review", content="eyes", **k):
        if reactions is not None:
            reactions.append((kind, comment_id))
        return comment_id in fresh

    async def _launch(api_key, prompt, repo_url, ref, **k):
        if launches is not None:
            launches.append({"repo_url": repo_url, "ref": ref, "prompt": prompt,
                             "pr_url": k.get("pr_url"),
                             "work_on_current_branch": k.get("work_on_current_branch")})
        return {"id": "stub-agent"}

    monkeypatch.setattr(T.cursor_client, "pr_for_agent", _pra)
    monkeypatch.setattr(T.cursor_client, "launch_agent", _launch)
    monkeypatch.setattr(T.github_client, "get_pr", _get_pr)
    monkeypatch.setattr(T.github_client, "check_runs", _checks)
    monkeypatch.setattr(T.github_client, "pr_reviews", _reviews)
    monkeypatch.setattr(T.github_client, "pr_review_comments", _comments)
    monkeypatch.setattr(T.github_client, "issue_comments", _issue_comments)
    monkeypatch.setattr(T.github_client, "add_reaction", _add_reaction)
    monkeypatch.setattr(T.github_client, "open_pr_for_branch", _open_pr)


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


async def test_tocursor_to_prprep_via_github_fallback(monkeypatch):
    # Cursor reports the run terminal with NO prUrl (a known Cursor gap), but the
    # agent's branch has an open PR on GitHub -> the tracker should still advance.
    svc = FakeService({"_id": "i1", "status": "tocursor", "cursorAgentId": "a1", "title": "t"})
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": None, "branch": "cursor/fix", "repoUrl": "https://github.com/o/r",
                      "runStatus": "FINISHED", "agentStatus": "FINISHED"},
        pr_by_branch={"html_url": "https://github.com/o/r/pull/77", "number": 77},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="tocursor"))[0])
    assert svc.issues["i1"]["status"] == "prprep"
    assert svc.issues["i1"]["prUrl"] == "https://github.com/o/r/pull/77"
    assert svc.issues["i1"]["prNumber"] == 77


async def test_tocursor_needs_human_when_no_pr_on_cursor_or_github(monkeypatch):
    # Terminal, branch known, but GitHub also has no PR -> genuinely no PR -> escalate.
    svc = FakeService({"_id": "i1", "status": "tocursor", "cursorAgentId": "a1", "title": "t"})
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": None, "branch": "cursor/fix", "repoUrl": "https://github.com/o/r",
                      "runStatus": "FINISHED", "agentStatus": "FINISHED"},
        pr_by_branch=None,
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="tocursor"))[0])
    assert svc.issues["i1"]["status"] == "needs_human"


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


async def test_prprep_closed_unmerged_to_needs_human(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        # PR closed without merging, CI green on the stale head -> must NOT promote to
        # pr_ready; the dead PR escalates to a human instead.
        get_pr={"merged": False, "state": "closed", "head": {"sha": "s1"}},
        check_runs=[{"name": "t", "status": "completed", "conclusion": "success"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert svc.issues["i2"]["status"] == "needs_human"
    assert launches == []  # no refix dispatched against a dead PR


async def test_pr_ready_closed_unmerged_to_needs_human(monkeypatch):
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "state": "closed", "head": {"sha": "s9"}},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "needs_human"


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
    # Updates the EXISTING PR in place (commits to its branch) — not a new PR.
    assert launches[0]["pr_url"] == PR_URL
    assert launches[0]["work_on_current_branch"] is True
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


def _capture_notify(monkeypatch, tr):
    """Capture the Slack thread messages the tracker posts via ``_notify``."""
    notes: list[str] = []

    async def _cap(issue, text):
        notes.append(text)

    monkeypatch.setattr(tr, "_notify", _cap)
    return notes


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
    notes = _capture_notify(monkeypatch, tr)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert len(launches) == 1  # CodeRabbit blocking triggers a refix even when CI is green
    assert "fix the edge case" in launches[0]["prompt"]
    # Slack thread is told we noticed CodeRabbit and are responding, with a preview.
    assert len(notes) == 1
    assert "Responding to CodeRabbit's requested changes" in notes[0]
    assert "fix the edge case" in notes[0]


# ---------------- prprep (merge conflict) ----------------

async def test_prprep_conflict_dispatches_resolution(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s1"}, "base": {"ref": "canary"},
                "mergeable_state": "dirty"},
        # CI would be green — the conflict must still win and trigger a resolution.
        check_runs=[{"name": "t", "status": "completed", "conclusion": "success"}],
    )
    tr = T.CursorTracker(svc)
    notes = _capture_notify(monkeypatch, tr)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert len(launches) == 1                       # conflict beats green CI
    assert launches[0]["ref"] == "b"                # resolve from the PR branch
    assert launches[0]["pr_url"] == PR_URL          # updates the existing PR…
    assert launches[0]["work_on_current_branch"] is True  # …on its own branch
    assert "merge conflict" in launches[0]["prompt"]
    assert "canary" in launches[0]["prompt"]        # base branch named for the agent
    assert svc.issues["i2"]["status"] == "prprep"   # stays, keeps watching
    assert svc.issues["i2"]["fixAttempts"] == 1
    assert svc.issues["i2"]["lastFixedSha"] == "s1"
    # Slack thread is told a conflict was detected and the PR went back to Cursor.
    assert notes and "Merge conflict with `canary`" in notes[0] and "back to Cursor" in notes[0]


async def test_prprep_conflict_dispatches_when_agent_stale(monkeypatch):
    # The original agent is hung ACTIVE for ~years (createdAt long past) -> treated as
    # terminal so a base-introduced conflict isn't deferred to a dead agent forever.
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "RUNNING",
                      "agentStatus": "ACTIVE", "createdAt": "2020-01-01T00:00:00Z"},
        get_pr={"merged": False, "head": {"sha": "s1"}, "base": {"ref": "canary"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert len(launches) == 1                       # hung agent no longer blocks resolution
    assert svc.issues["i2"]["fixAttempts"] == 1


async def test_prprep_conflict_skips_same_sha(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue(lastFixedSha="s1", fixAttempts=1))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s1"}, "base": {"ref": "canary"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []                           # already resolving s1
    assert svc.issues["i2"]["fixAttempts"] == 1


async def test_prprep_conflict_waits_while_agent_pushing(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "RUNNING",
                      "agentStatus": "RUNNING"},
        get_pr={"merged": False, "head": {"sha": "s1"}, "base": {"ref": "canary"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []                           # agent still pushing -> don't pile on
    assert svc.issues["i2"]["status"] == "prprep"


async def test_prprep_conflict_caps_at_max_attempts(monkeypatch):
    launches: list = []
    svc = FakeService(_prprep_issue(fixAttempts=3))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s2"}, "base": {"ref": "canary"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="prprep"))[0])
    assert launches == []
    assert svc.issues["i2"]["status"] == "needs_human"


async def test_pr_ready_conflict_returns_to_prprep(monkeypatch):
    launches: list = []
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}, "base": {"ref": "main"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "prprep"   # pulled back to resolve
    assert len(launches) == 1
    assert "main" in launches[0]["prompt"]
    assert svc.issues["i3"]["fixAttempts"] == 1
    assert svc.issues["i3"]["lastFixedSha"] == "s9"


async def test_pr_ready_conflict_defers_to_active_agent(monkeypatch):
    # A conflicted pr_ready PR whose agent is still ACTIVE must NOT dispatch a duplicate
    # resolver — the live agent may resolve the conflict itself.
    launches: list = []
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "RUNNING",
                      "agentStatus": "RUNNING"},
        get_pr={"merged": False, "head": {"sha": "s9"}, "base": {"ref": "main"},
                "mergeable_state": "dirty"},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert launches == []                            # deferred to the live agent
    assert svc.issues["i3"]["status"] == "pr_ready"  # no duplicate dispatch


# ---------------- pr_ready (late human review) ----------------

def _pr_ready_issue(**over):
    base = {"_id": "i3", "status": "pr_ready", "cursorAgentId": "a1", "prUrl": PR_URL,
            "prBranch": "b", "prNumber": 5, "title": "t", "fixAttempts": 0}
    base.update(over)
    return base

HUMAN_COMMENT = {"id": 901, "user": {"type": "User", "login": "alice"},
                 "body": "please rename this", "created_at": "2026-06-15T22:00:00Z"}


async def test_pr_ready_human_comment_returns_to_prprep(monkeypatch):
    launches: list = []
    svc = FakeService(_pr_ready_issue())  # no lastHumanCommentAt -> the comment is new
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}},
        issue_comments=[HUMAN_COMMENT],
    )
    tr = T.CursorTracker(svc)
    notes = _capture_notify(monkeypatch, tr)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "prprep"
    assert svc.issues["i3"]["fixAttempts"] == 1
    assert svc.issues["i3"]["lastFixedSha"] == "s9"
    # high-water mark advanced past the comment we just acted on
    assert svc.issues["i3"]["lastHumanCommentAt"] == "2026-06-15T22:00:00Z"
    assert len(launches) == 1                   # dispatched a fix for the review
    assert "please rename this" in launches[0]["prompt"]
    # Slack thread is told we noticed the reviewer's comment and are responding.
    assert len(notes) == 1
    assert "Responding to reviewer comments" in notes[0]
    assert "@alice" in notes[0] and "please rename this" in notes[0]


async def test_pr_ready_pickup_is_independent_of_reactions(monkeypatch):
    # Detection is purely by the high-water mark — pickup must work without any
    # reaction call. (Regression for the bug where a forbidden 👀 silently dropped
    # team comments before reactions were removed entirely.)
    launches: list = []
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}},
        issue_comments=[HUMAN_COMMENT],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "prprep"
    assert len(launches) == 1


async def test_pr_ready_no_new_comments_stays_ready(monkeypatch):
    launches: list = []
    # high-water mark is at/after the comment -> nothing new
    svc = FakeService(_pr_ready_issue(lastHumanCommentAt="2026-06-15T23:00:00Z"))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}},
        issue_comments=[HUMAN_COMMENT],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "pr_ready"  # no change
    assert launches == []


async def test_pr_ready_ignores_bot_comments(monkeypatch):
    launches: list = []
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}},
        # CodeRabbit/Cursor are type "Bot" -> never treated as human feedback.
        issue_comments=[{"id": 5, "user": {"type": "Bot", "login": CR}, "body": "nit",
                         "created_at": "2026-06-15T22:00:00Z"}],
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "pr_ready"  # bot comment -> no change
    assert launches == []


async def test_pr_ready_merged_closes(monkeypatch):
    svc = FakeService(_pr_ready_issue())
    _patch_clients(
        monkeypatch,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": True, "head": {"sha": "s9"}},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "closed"


async def test_pr_ready_human_comment_at_cap_escalates(monkeypatch):
    launches: list = []
    svc = FakeService(_pr_ready_issue(fixAttempts=3))
    _patch_clients(
        monkeypatch, launches=launches,
        pr_for_agent={"prUrl": PR_URL, "branch": "b", "runStatus": "FINISHED",
                      "agentStatus": "FINISHED"},
        get_pr={"merged": False, "head": {"sha": "s9"}},
        issue_comments=[HUMAN_COMMENT], new_comment_ids={901},
    )
    tr = T.CursorTracker(svc)
    await tr._track_one((await svc.list("issues", field="status", value="pr_ready"))[0])
    assert svc.issues["i3"]["status"] == "needs_human"
    assert launches == []
