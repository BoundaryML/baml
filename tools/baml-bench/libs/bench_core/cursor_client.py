"""Minimal Cursor Cloud Agents API client.

Launches a background agent on a GitHub repo to work a fix. Slack can't be used
to trigger Cursor (it ignores app/API-posted mentions), so the fixer calls this
API directly. Auth is HTTP Basic with the API key as the username and an empty
password (Cursor's `-u API_KEY:` convention).
"""

from __future__ import annotations

import base64
import os
from typing import Any, Optional

import httpx

CURSOR_API_BASE = os.environ.get("CURSOR_API_BASE", "https://api.cursor.com")


async def launch_agent(
    api_key: str,
    prompt_text: str,
    repo_url: str,
    ref: str,
    *,
    agent_id: Optional[str] = None,
    auto_create_pr: bool = True,
    model: Optional[str] = None,
    timeout: float = 60.0,
) -> dict[str, Any]:
    """Launch a Cursor cloud agent to work a fix on a GitHub repo.

    Posts to the Cursor Cloud Agents API (`POST /v1/agents`) with HTTP Basic auth
    (the API key as the username, empty password). When `agent_id` is supplied it
    is sent as the request's `agentId`; reusing the same id makes the launch
    idempotent (Cursor returns 409 `agent_id_conflict`, which is treated here as
    "already launched" so a crashed/retried dispatch never spawns a duplicate).

    Args:
        api_key: Cursor API key used for HTTP Basic auth.
        prompt_text: Instruction text handed to the agent.
        repo_url: GitHub repository the agent works in.
        ref: Git ref the agent branches from (its starting ref).
        agent_id: Optional client-supplied, stable agent id for idempotent launch.
        auto_create_pr: Whether the agent opens a pull request with its changes.
        model: Optional Cursor model id; omitted to use the account default.
        timeout: HTTP timeout in seconds for the launch request.

    Returns:
        The created agent object (includes `id` and `latestRunId`) on a fresh
        launch, or `{"id": agent_id, "alreadyLaunched": True}` when the id was
        already used (409 `agent_id_conflict`).

    Raises:
        httpx.HTTPStatusError: If the Cursor API returns a non-2xx response, other
            than a 409 whose body is `error.code == "agent_id_conflict"`.
    """
    auth = base64.b64encode(f"{api_key}:".encode()).decode()
    body: dict[str, Any] = {
        "prompt": {"text": prompt_text},
        "repos": [{"url": repo_url, "startingRef": ref}],
        "autoCreatePR": auto_create_pr,
    }
    if agent_id:
        body["agentId"] = agent_id
    if model:
        body["model"] = {"id": model}
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.post(
            f"{CURSOR_API_BASE}/v1/agents",
            json=body,
            headers={"Authorization": f"Basic {auth}", "Content-Type": "application/json"},
        )
        # A reused agentId (409 agent_id_conflict) means this issue was already
        # dispatched (an idempotent re-dispatch after a crash); treat it as already
        # launched. Other 409s (e.g. agent_busy) or an unparseable body must
        # surface, not be silently swallowed.
        if r.status_code == 409 and agent_id:
            try:
                body = r.json()
            except Exception:  # noqa: BLE001
                body = None
            err = body.get("error") if isinstance(body, dict) else None
            if isinstance(err, dict) and err.get("code") == "agent_id_conflict":
                return {"id": agent_id, "alreadyLaunched": True}
        r.raise_for_status()
        return r.json()
