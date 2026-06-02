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
