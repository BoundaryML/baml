"""Unit tests for the hard-task generator (``services/cron/generator.py``) and the
cron cycle's generate-or-fall-back-to-static selection (``services/cron/__main__``).
"""

from __future__ import annotations

import json

from services.cron import __main__ as cron
from services.cron import generator as g


class FakeResult:
    def __init__(self, post_files=None, transcript=""):
        self.post_files = post_files or {}
        self.transcript = transcript


class FakeProxy:
    def __init__(self, result):
        self._r = result

    async def run_agent(self, req, timeout=None):
        self.req = req
        return self._r


class FakeService:
    def __init__(self, recent=None):
        self.recent = recent or []

    async def list(self, table, **kw):
        return self.recent


# ---------------- generator ----------------

async def test_generate_parses_tasks_json():
    tasks = {"tasks": ["Implement `function foo(x: int) -> int` …", "Implement `function bar` …"]}
    proxy = FakeProxy(FakeResult(post_files={"tasks.json": json.dumps(tasks)}))
    out = await g.generate_hard_tasks(proxy, recent_prompts=["old"], count=3)
    assert out == tasks["tasks"]


async def test_generate_caps_to_count():
    proxy = FakeProxy(FakeResult(post_files={"tasks.json": json.dumps({"tasks": ["a", "b", "c"]})}))
    assert await g.generate_hard_tasks(proxy, [], count=2) == ["a", "b"]


async def test_generate_filters_blank_and_nonstring():
    tasks = {"tasks": ["good", "  ", "", None, "also good"]}
    proxy = FakeProxy(FakeResult(post_files={"tasks.json": json.dumps(tasks)}))
    assert await g.generate_hard_tasks(proxy, [], count=10) == ["good", "also good"]


async def test_generate_falls_back_to_transcript_scrape():
    proxy = FakeProxy(FakeResult(post_files={}, transcript='noise {"tasks": ["t1"]} tail'))
    assert await g.generate_hard_tasks(proxy, [], count=3) == ["t1"]


async def test_generate_empty_on_garbage():
    proxy = FakeProxy(FakeResult(post_files={"tasks.json": "not json"}, transcript="no json"))
    assert await g.generate_hard_tasks(proxy, [], count=3) == []


# ---------------- cron selection (generate vs static fallback) ----------------

async def test_cycle_uses_generated_tasks(monkeypatch):
    async def fake_gen(proxy, recent, count):
        return ["gen-1", "gen-2"]
    monkeypatch.setattr(cron, "GENERATE_TASKS", True)
    monkeypatch.setattr(cron, "generate_hard_tasks", fake_gen)
    out = await cron._cycle_tasks(FakeService([{"prompt": "p"}]), proxy=object())
    assert out == ["gen-1", "gen-2"]


async def test_cycle_falls_back_when_generation_empty(monkeypatch):
    async def fake_gen(proxy, recent, count):
        return []
    monkeypatch.setattr(cron, "GENERATE_TASKS", True)
    monkeypatch.setattr(cron, "generate_hard_tasks", fake_gen)
    out = await cron._cycle_tasks(FakeService(), proxy=object())
    assert out and all(isinstance(p, str) and p for p in out)  # static rotation


async def test_cycle_falls_back_when_generation_raises(monkeypatch):
    async def fake_gen(proxy, recent, count):
        raise RuntimeError("boom")
    monkeypatch.setattr(cron, "GENERATE_TASKS", True)
    monkeypatch.setattr(cron, "generate_hard_tasks", fake_gen)
    out = await cron._cycle_tasks(FakeService(), proxy=object())
    assert out and all(isinstance(p, str) and p for p in out)


async def test_cycle_static_when_no_proxy(monkeypatch):
    monkeypatch.setattr(cron, "GENERATE_TASKS", True)
    out = await cron._cycle_tasks(FakeService(), proxy=None)
    assert out and all(isinstance(p, str) and p for p in out)
