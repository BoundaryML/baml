"""Unit tests for FixDispatch's re-dispatch decision
(``services/notion_fixer/__main__.py`` :meth:`FixDispatch._prior_dispatch_alive`).

A human re-approving a previously-dispatched issue must launch a FRESH agent when
the prior one is dead (terminal/expired with no PR), but must NOT duplicate one that
is still working or has already opened a PR.
"""

from __future__ import annotations

from services.notion_fixer import __main__ as nf


def _stub_pr(monkeypatch, value=None, raises=False):
    async def f(api_key, ref, **k):
        if raises:
            raise RuntimeError("agent 404")
        return value
    monkeypatch.setattr(nf.cursor_client, "pr_for_agent", f)


async def test_alive_when_agent_has_pr(monkeypatch):
    _stub_pr(monkeypatch, {"prUrl": "https://github.com/o/r/pull/1", "runStatus": "RUNNING"})
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is True


async def test_alive_when_agent_still_running(monkeypatch):
    _stub_pr(monkeypatch, {"prUrl": None, "runStatus": "RUNNING", "agentStatus": "RUNNING"})
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is True


async def test_dead_when_finished_without_pr(monkeypatch):
    _stub_pr(monkeypatch, {"prUrl": None, "runStatus": "FINISHED", "agentStatus": "FINISHED"})
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is False


async def test_dead_when_expired_without_pr(monkeypatch):
    _stub_pr(monkeypatch, {"prUrl": None, "runStatus": "EXPIRED", "agentStatus": "EXPIRED"})
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is False


async def test_dead_when_no_run(monkeypatch):
    _stub_pr(monkeypatch, None)
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is False


async def test_dead_when_unreachable(monkeypatch):
    _stub_pr(monkeypatch, raises=True)
    assert await nf.FixDispatch._prior_dispatch_alive("bc-1") is False
