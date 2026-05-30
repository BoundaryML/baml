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
    auto_create_pr: bool = True,
    model: Optional[str] = None,
    timeout: float = 60.0,
) -> dict[str, Any]:
    """Launch a Cursor cloud agent to work a fix on a GitHub repo.

    Posts to the Cursor Cloud Agents API (`POST /v1/agents`) with HTTP Basic auth
    (the API key as the username, empty password).

    Args:
        api_key: Cursor API key used for HTTP Basic auth.
        prompt_text: Instruction text handed to the agent.
        repo_url: GitHub repository the agent works in.
        ref: Git ref the agent branches from (its starting ref).
        auto_create_pr: Whether the agent opens a pull request with its changes.
        model: Optional Cursor model id; omitted to use the account default.
        timeout: HTTP timeout in seconds for the launch request.

    Returns:
        The created agent object (includes `id` and `latestRunId`).

    Raises:
        httpx.HTTPStatusError: If the Cursor API returns a non-2xx response.
    """
    auth = base64.b64encode(f"{api_key}:".encode()).decode()
    body: dict[str, Any] = {
        "prompt": {"text": prompt_text},
        "repos": [{"url": repo_url, "startingRef": ref}],
        "autoCreatePR": auto_create_pr,
    }
    if model:
        body["model"] = {"id": model}
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.post(
            f"{CURSOR_API_BASE}/v1/agents",
            json=body,
            headers={"Authorization": f"Basic {auth}", "Content-Type": "application/json"},
        )
        r.raise_for_status()
        return r.json()
