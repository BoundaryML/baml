"""Async gateway to Convex via its HTTP function API.

We deliberately do NOT use the synchronous `convex` Python client here: in a
high-concurrency async server its websocket-based query (bound to its
creating thread, one /api/sync socket) hangs and churns connections. The
HTTP function API (POST /api/{query,mutation,action}) is plain request/
response — fast, stateless, and trivially concurrent under httpx.

Subscriptions aren't exposed over HTTP, so `subscribe_counts` polls the
count query on a short interval and yields on change (the SSE wake signal).
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Any, AsyncIterator, Optional

import httpx

log = logging.getLogger("convex")


class ConvexGateway:
    """Async client for Convex's HTTP function API (query/mutation/action)."""

    def __init__(self, url: str, admin_key: Optional[str] = None, poll_interval: float = 2.0):
        """Initialize the gateway and its underlying HTTP client.

        Args:
            url: Base Convex deployment URL (trailing slash trimmed).
            admin_key: Optional Convex admin key for the Authorization header.
            poll_interval: Seconds between polls in subscribe_counts.
        """
        self._url = url.rstrip("/")
        self._poll = poll_interval
        headers = {"Content-Type": "application/json"}
        if admin_key:
            headers["Authorization"] = f"Convex {admin_key}"
        self._client = httpx.AsyncClient(timeout=30.0, headers=headers)

    async def _run(self, kind: str, name: str, args: dict[str, Any]) -> Any:
        """Invoke a Convex function over the HTTP API and return its value.

        Args:
            kind: Function kind: ``query``, ``mutation``, or ``action``.
            name: Convex function path (e.g. ``tasks:create``).
            args: Argument object passed to the function.

        Returns:
            The function's return value.

        Raises:
            RuntimeError: When Convex reports a non-success status.
            httpx.HTTPStatusError: When the HTTP request itself fails.
        """
        r = await self._client.post(
            f"{self._url}/api/{kind}", json={"path": name, "args": args, "format": "json"}
        )
        r.raise_for_status()
        data = r.json()
        if data.get("status") != "success":
            raise RuntimeError(
                f"convex {kind} {name} failed: {data.get('errorMessage') or data}"
            )
        return data.get("value")

    async def query(self, name: str, args: dict[str, Any]) -> Any:
        """Run a Convex query function.

        Args:
            name: Convex query path (e.g. ``tasks:get``).
            args: Argument object passed to the query.

        Returns:
            The query's return value.
        """
        return await self._run("query", name, args)

    async def mutation(self, name: str, args: dict[str, Any]) -> Any:
        """Run a Convex mutation function.

        Args:
            name: Convex mutation path (e.g. ``tasks:create``).
            args: Argument object passed to the mutation.

        Returns:
            The mutation's return value.
        """
        return await self._run("mutation", name, args)

    async def action(self, name: str, args: dict[str, Any]) -> Any:
        """Run a Convex action function.

        Args:
            name: Convex action path.
            args: Argument object passed to the action.

        Returns:
            The action's return value.
        """
        return await self._run("action", name, args)

    async def subscribe_counts(self, name: str, args: dict[str, Any]) -> AsyncIterator[int]:
        """Poll a count query and yield whenever the count changes.

        Polls on the configured interval forever; transient query failures are
        logged and retried rather than propagated.

        Args:
            name: Convex count query path (e.g. ``tasks:countClaimable``).
            args: Argument object passed to the query.

        Yields:
            The latest integer count, emitted only when it differs from the
            previously yielded value.
        """
        last: Optional[int] = None
        while True:
            try:
                value = await self._run("query", name, args)
                count = int(value)
                if count != last:
                    last = count
                    yield count
            except Exception:  # noqa: BLE001
                log.debug("count poll failed for %s", name, exc_info=True)
            await asyncio.sleep(self._poll)


def gateway_from_env() -> ConvexGateway:
    """Construct a ConvexGateway from environment variables.

    Reads CONVEX_URL, CONVEX_ADMIN_KEY, and CONVEX_POLL_INTERVAL_SECS.

    Returns:
        A ConvexGateway configured from the environment.
    """
    return ConvexGateway(
        url=os.environ["CONVEX_URL"],
        admin_key=os.environ.get("CONVEX_ADMIN_KEY"),
        poll_interval=float(os.environ.get("CONVEX_POLL_INTERVAL_SECS", "2.0")),
    )
