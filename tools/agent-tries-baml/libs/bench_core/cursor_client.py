"""Minimal Cursor Cloud Agents API client.

Launches a background agent on a GitHub repo to work a fix. Slack can't be used
to trigger Cursor (it ignores app/API-posted mentions), so the fixer calls this
API directly. Auth is HTTP Basic with the API key as the username and an empty
password (Cursor's `-u API_KEY:` convention).
"""

from __future__ import annotations

import base64
import logging
import os
from typing import Any, Optional

import httpx

log = logging.getLogger("cursor_client")

# Default Cursor API base. launch_agent re-reads CURSOR_API_BASE at call time (falling
# back to this), so a value set after import - e.g. tests pointing it at a stub - is
# honored rather than frozen at module-import time.
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
    pr_url: Optional[str] = None,
    work_on_current_branch: bool = False,
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
        pr_url: When set, the agent works on this existing PR's head branch (Cursor
            ignores `ref`/`startingRef`) — used to UPDATE an existing PR (e.g. resolve
            its conflict / fix its CI) instead of opening a brand-new one.
        work_on_current_branch: When True, the agent commits directly to the starting
            branch instead of cutting a fresh `cursor/...` branch. Pair with `pr_url`
            (or a `ref` that is the PR's head branch) so the existing PR is updated.
        timeout: HTTP timeout in seconds for the launch request.

    Returns:
        The created agent object (includes `id` and `latestRunId`) on a fresh
        launch, or `{"id": agent_id, "alreadyLaunched": True}` when the id was
        already used (409 `agent_id_conflict`).

    Raises:
        httpx.HTTPStatusError: If the Cursor API returns a non-2xx response, other
            than a 409 whose body is `error.code == "agent_id_conflict"`.
    """
    base = os.environ.get("CURSOR_API_BASE", CURSOR_API_BASE)
    auth = base64.b64encode(f"{api_key}:".encode()).decode()
    repo: dict[str, Any] = {"url": repo_url, "startingRef": ref}
    if pr_url:
        # Target an existing PR: Cursor works on that PR's head branch and ignores
        # startingRef, so commits land on the existing PR instead of a new branch.
        repo["prUrl"] = pr_url
    body: dict[str, Any] = {
        "prompt": {"text": prompt_text},
        "repos": [repo],
        "autoCreatePR": auto_create_pr,
    }
    if work_on_current_branch:
        body["workOnCurrentBranch"] = True
    if agent_id:
        body["agentId"] = agent_id
    if model:
        body["model"] = {"id": model}
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.post(
            f"{base}/v1/agents",
            json=body,
            headers={"Authorization": f"Basic {auth}", "Content-Type": "application/json"},
        )
        # A reused agentId (409 agent_id_conflict) means this issue was already
        # dispatched (an idempotent re-dispatch after a crash); treat it as already
        # launched. Other 409s (e.g. agent_busy) or an unparseable body must
        # surface, not be silently swallowed.
        if r.status_code == 409 and agent_id:
            try:
                resp_body = r.json()
            except Exception:  # noqa: BLE001
                resp_body = None
            err = resp_body.get("error") if isinstance(resp_body, dict) else None
            if isinstance(err, dict) and err.get("code") == "agent_id_conflict":
                return {"id": agent_id, "alreadyLaunched": True}
        if r.status_code >= 400:
            # Surface Cursor's error body (request payload it rejected) so a 400
            # is diagnosable instead of an opaque status code.
            log.error(
                "cursor /v1/agents %s: %s | sent repos=%s ref=%s agentId=%s model=%s",
                r.status_code, (r.text or "")[:600],
                body.get("repos"), ref, body.get("agentId"), body.get("model"),
            )
        r.raise_for_status()
        return r.json()


# Cursor run statuses that mean the run is over (the agent stopped working).
TERMINAL_RUN_STATUSES = frozenset({"FINISHED", "ERROR", "CANCELLED", "EXPIRED"})


def _auth_header(api_key: str) -> dict[str, str]:
    """Build the HTTP Basic auth header Cursor expects (API key as username).

    Args:
        api_key: Cursor API key.

    Returns:
        A headers dict carrying the Basic Authorization value.
    """
    auth = base64.b64encode(f"{api_key}:".encode()).decode()
    return {"Authorization": f"Basic {auth}"}


async def get_agent(api_key: str, agent_id: str, *, timeout: float = 30.0) -> dict[str, Any]:
    """Fetch a Cursor agent's current state (``GET /v1/agents/{id}``).

    Args:
        api_key: Cursor API key.
        agent_id: The agent id to look up.
        timeout: HTTP timeout in seconds.

    Returns:
        The agent object (includes ``status`` and ``latestRunId``).

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    base = os.environ.get("CURSOR_API_BASE", CURSOR_API_BASE)
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(f"{base}/v1/agents/{agent_id}", headers=_auth_header(api_key))
        r.raise_for_status()
        return r.json()


async def get_run(api_key: str, agent_id: str, run_id: str, *,
                  timeout: float = 30.0) -> dict[str, Any]:
    """Fetch one run of an agent (``GET /v1/agents/{id}/runs/{runId}``).

    The run object carries ``status`` and ``git.branches[]`` entries shaped
    ``{repoUrl, branch?, prUrl?}`` — one per branch the agent has pushed.

    Args:
        api_key: Cursor API key.
        agent_id: The owning agent id.
        run_id: The run id to fetch.
        timeout: HTTP timeout in seconds.

    Returns:
        The run object.

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    base = os.environ.get("CURSOR_API_BASE", CURSOR_API_BASE)
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(
            f"{base}/v1/agents/{agent_id}/runs/{run_id}", headers=_auth_header(api_key)
        )
        r.raise_for_status()
        return r.json()


async def pr_for_agent(api_key: str, agent_id: str, *,
                       timeout: float = 30.0) -> Optional[dict[str, Any]]:
    """Resolve an agent's latest run into its PR (if it has opened one yet).

    Reads the agent (for its ``latestRunId`` + ``status``), then the latest run,
    and returns the first pushed branch carrying a ``prUrl``.

    Args:
        api_key: Cursor API key.
        agent_id: The agent id to resolve.
        timeout: HTTP timeout in seconds.

    Returns:
        ``{prUrl, branch, repoUrl, runStatus, agentStatus, createdAt}`` when a PR
        exists, or ``{prUrl: None, ..., runStatus, agentStatus, createdAt}`` when the
        agent has no PR yet (so callers can still read the run/agent status and the
        agent's age); None only when the agent has no run at all.

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    agent = await get_agent(api_key, agent_id, timeout=timeout)
    run_id = agent.get("latestRunId")
    agent_status = agent.get("status")
    created_at = agent.get("createdAt")
    if not run_id:
        return None
    run = await get_run(api_key, agent_id, run_id, timeout=timeout)
    run_status = run.get("status")
    branches = ((run.get("git") or {}).get("branches")) or []
    for b in branches:
        if b.get("prUrl"):
            return {"prUrl": b["prUrl"], "branch": b.get("branch"),
                    "repoUrl": b.get("repoUrl"), "runStatus": run_status,
                    "agentStatus": agent_status, "createdAt": created_at}
    # No PR yet — still surface the run/agent status (and any branch) so the
    # tracker can decide whether to keep waiting or escalate.
    first = branches[0] if branches else {}
    return {"prUrl": None, "branch": first.get("branch"), "repoUrl": first.get("repoUrl"),
            "runStatus": run_status, "agentStatus": agent_status, "createdAt": created_at}


async def add_followup(api_key: str, agent_id: str, text: str, *,
                       mode: str = "agent", timeout: float = 60.0) -> dict[str, Any]:
    """Add a follow-up run to an existing agent (``POST /v1/agents/{id}/runs``).

    Keeps the agent's existing branch/PR and context — the agent works the new
    instruction and pushes more commits to the same PR.

    Args:
        api_key: Cursor API key.
        agent_id: The agent to continue.
        text: The follow-up instruction.
        mode: Conversation mode for this run (``agent`` or ``plan``).
        timeout: HTTP timeout in seconds.

    Returns:
        The created run object (includes ``id`` and ``status``).

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    base = os.environ.get("CURSOR_API_BASE", CURSOR_API_BASE)
    body: dict[str, Any] = {"prompt": {"text": text}, "mode": mode}
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.post(
            f"{base}/v1/agents/{agent_id}/runs",
            json=body,
            headers={**_auth_header(api_key), "Content-Type": "application/json"},
        )
        r.raise_for_status()
        return r.json()
