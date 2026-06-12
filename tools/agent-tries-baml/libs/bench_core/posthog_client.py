"""Minimal PostHog query client (HogQL over the query API).

Backs @bammy's analytics route: a natural-language question becomes a HogQL
query (written by an agent) and runs here. Read-only by construction — only
the /query endpoint is used.

Env contract (Infisical, ATB_-prefixed like the other bot secrets):
  ATB_POSTHOG_API_KEY     personal API key with query:read scope (phx_...)
  ATB_POSTHOG_PROJECT_ID  numeric project id
  ATB_POSTHOG_HOST        optional, defaults to https://us.posthog.com
"""

from __future__ import annotations

import os
from typing import Any, Optional

import httpx


def configured() -> bool:
    """Whether the PostHog env contract is satisfied.

    Returns:
        True when both the API key and project id are present.
    """
    return bool(os.environ.get("ATB_POSTHOG_API_KEY")
                and os.environ.get("ATB_POSTHOG_PROJECT_ID"))


async def hogql(query: str, *, timeout: float = 60.0) -> dict[str, Any]:
    """Run one HogQL query against the configured project.

    Args:
        query: The HogQL statement (SELECT ... — the API rejects mutations).
        timeout: Per-request timeout in seconds.

    Returns:
        ``{"columns": [...], "results": [[...], ...]}``.

    Raises:
        RuntimeError: When unconfigured, or when PostHog rejects the query
            (the message carries PostHog's error detail for the repair loop).
        httpx.HTTPError: On transport failures.
    """
    if not configured():
        raise RuntimeError("PostHog is not configured (ATB_POSTHOG_API_KEY / ATB_POSTHOG_PROJECT_ID)")
    host = (os.environ.get("ATB_POSTHOG_HOST") or "https://us.posthog.com").rstrip("/")
    project = os.environ["ATB_POSTHOG_PROJECT_ID"]
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.post(
            f"{host}/api/projects/{project}/query/",
            json={"query": {"kind": "HogQLQuery", "query": query}},
            headers={"Authorization": f"Bearer {os.environ['ATB_POSTHOG_API_KEY']}"},
        )
        if r.status_code >= 400:
            detail: Any
            try:
                detail = r.json().get("detail") or r.json()
            except Exception:  # noqa: BLE001
                detail = r.text[:300]
            raise RuntimeError(f"PostHog rejected the query ({r.status_code}): {detail}")
        data = r.json()
    return {"columns": data.get("columns") or [], "results": data.get("results") or []}


async def top_events(days: int = 30, limit: int = 40) -> list[tuple[str, int]]:
    """The project's most frequent event names — schema context for the agent.

    Args:
        days: Lookback window.
        limit: Maximum number of event names.

    Returns:
        (event, count) tuples, most frequent first; empty on any failure
        (context is nice-to-have, never load-bearing).
    """
    try:
        out = await hogql(
            f"SELECT event, count() AS n FROM events "
            f"WHERE timestamp > now() - INTERVAL {int(days)} DAY "
            f"GROUP BY event ORDER BY n DESC LIMIT {int(limit)}"
        )
        return [(row[0], row[1]) for row in out["results"]]
    except Exception:  # noqa: BLE001
        return []


def format_table(columns: list[Any], results: list[list[Any]],
                 *, max_rows: int = 20, max_width: int = 36) -> str:
    """Render query results as an aligned monospace table for Slack.

    Args:
        columns: Column names from the query response.
        results: Row values.
        max_rows: Rows shown before truncation.
        max_width: Per-cell character cap.

    Returns:
        A plain-text table (caller wraps it in a code block).
    """
    if not results:
        return "(no rows)"

    def cell(v: Any) -> str:
        s = "" if v is None else str(v)
        return s[: max_width - 1] + "…" if len(s) > max_width else s

    headers = [cell(c) for c in (columns or [f"col{i}" for i in range(len(results[0]))])]
    rows = [[cell(v) for v in row] for row in results[:max_rows]]
    widths = [max(len(h), *(len(r[i]) for r in rows)) if rows else len(h)
              for i, h in enumerate(headers)]
    lines = ["  ".join(h.ljust(widths[i]) for i, h in enumerate(headers)),
             "  ".join("-" * w for w in widths)]
    lines += ["  ".join(r[i].ljust(widths[i]) for i in range(len(r))) for r in rows]
    if len(results) > max_rows:
        lines.append(f"... {len(results) - max_rows} more row(s)")
    return "\n".join(lines)
