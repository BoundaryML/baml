"""Unit tests for the Cursor cloud-agent client.

Cover that a supplied agentId is sent on the launch, that a 409
``agent_id_conflict`` is treated as an idempotent already-launched result (no
raise), and that other error statuses still raise. The Cursor API is mocked with
respx, so these run fast with no network.
"""

import json

import httpx
import pytest
import respx

from bench_core import cursor_client

AGENTS_URL = f"{cursor_client.CURSOR_API_BASE}/v1/agents"


@respx.mock
async def test_launch_agent_sends_agent_id_and_returns_agent():
    """A 200 launch posts the supplied agentId and returns the agent JSON."""
    route = respx.post(AGENTS_URL).mock(
        return_value=httpx.Response(200, json={"id": "agent-xyz", "latestRunId": "run-1"})
    )
    result = await cursor_client.launch_agent(
        "key", "fix it", "https://github.com/o/r", "canary",
        agent_id="notion-fixer-issue1",
    )
    assert result == {"id": "agent-xyz", "latestRunId": "run-1"}
    assert route.called
    sent = json.loads(route.calls.last.request.content)
    assert sent["agentId"] == "notion-fixer-issue1"


@respx.mock
async def test_launch_agent_409_is_already_launched():
    """A 409 agent_id_conflict returns a normalized already-launched result without raising."""
    respx.post(AGENTS_URL).mock(
        return_value=httpx.Response(409, json={"error": {"code": "agent_id_conflict"}})
    )
    result = await cursor_client.launch_agent(
        "key", "fix it", "https://github.com/o/r", "canary",
        agent_id="notion-fixer-issue1",
    )
    assert result == {"id": "notion-fixer-issue1", "alreadyLaunched": True}


@respx.mock
async def test_launch_agent_409_wrong_error_code_raises():
    """A 409 whose error.code is not agent_id_conflict is not idempotent and raises."""
    respx.post(AGENTS_URL).mock(
        return_value=httpx.Response(409, json={"error": {"code": "some_other_conflict"}})
    )
    with pytest.raises(httpx.HTTPStatusError):
        await cursor_client.launch_agent(
            "key", "fix it", "https://github.com/o/r", "canary",
            agent_id="notion-fixer-issue1",
        )


@respx.mock
async def test_launch_agent_non_409_error_still_raises():
    """A non-409 error status is not swallowed and still raises HTTPStatusError."""
    respx.post(AGENTS_URL).mock(return_value=httpx.Response(500, json={"error": "boom"}))
    with pytest.raises(httpx.HTTPStatusError):
        await cursor_client.launch_agent(
            "key", "fix it", "https://github.com/o/r", "canary",
            agent_id="notion-fixer-issue1",
        )


@respx.mock
async def test_pr_for_agent_resolves_pr_from_latest_run():
    """pr_for_agent reads the agent's latest run and returns the branch's PR url."""
    respx.get(f"{AGENTS_URL}/a1").mock(
        return_value=httpx.Response(200, json={"status": "RUNNING", "latestRunId": "r1"})
    )
    respx.get(f"{AGENTS_URL}/a1/runs/r1").mock(
        return_value=httpx.Response(200, json={"status": "FINISHED", "git": {"branches": [
            {"repoUrl": "https://github.com/o/r", "branch": "cursor/fix",
             "prUrl": "https://github.com/o/r/pull/7"}]}})
    )
    pr = await cursor_client.pr_for_agent("key", "a1")
    assert pr["prUrl"] == "https://github.com/o/r/pull/7"
    assert pr["branch"] == "cursor/fix"
    assert pr["runStatus"] == "FINISHED"
    assert pr["agentStatus"] == "RUNNING"


@respx.mock
async def test_pr_for_agent_no_pr_yet_surfaces_status():
    """With no PR pushed, pr_for_agent still returns the run/agent status (prUrl None)."""
    respx.get(f"{AGENTS_URL}/a2").mock(
        return_value=httpx.Response(200, json={"status": "FINISHED", "latestRunId": "r2"})
    )
    respx.get(f"{AGENTS_URL}/a2/runs/r2").mock(
        return_value=httpx.Response(200, json={"status": "FINISHED", "git": {"branches": []}})
    )
    pr = await cursor_client.pr_for_agent("key", "a2")
    assert pr["prUrl"] is None
    assert pr["runStatus"] == "FINISHED"


@respx.mock
async def test_add_followup_posts_a_new_run():
    """add_followup POSTs the instruction to /v1/agents/{id}/runs."""
    route = respx.post(f"{AGENTS_URL}/a1/runs").mock(
        return_value=httpx.Response(200, json={"id": "r9", "status": "CREATING"})
    )
    out = await cursor_client.add_followup("key", "a1", "fix the failing test")
    assert out == {"id": "r9", "status": "CREATING"}
    sent = json.loads(route.calls.last.request.content)
    assert sent["prompt"]["text"] == "fix the failing test"
