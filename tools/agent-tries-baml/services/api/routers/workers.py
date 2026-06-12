"""Worker presence endpoints for the dashboard's live agents roster.

Every long-lived processor POSTs a heartbeat here (~15s) via
bench_core.processor; the UI reads GET /workers through /api/state.
Presence is observability only — never load-bearing for queue correctness.
"""

from __future__ import annotations

from typing import Any, Optional

from fastapi import APIRouter, Response
from pydantic import BaseModel

from ..convex_gateway import ConvexGateway


class HeartbeatBody(BaseModel):
    """Request body for a worker presence heartbeat."""

    workerId: str
    role: str
    status: str  # idle | busy
    currentItemId: Optional[str] = None


def make_workers_router(convex: ConvexGateway) -> APIRouter:
    """Build the worker-presence router.

    Args:
        convex: Gateway used to invoke the workers Convex functions.

    Returns:
        An APIRouter exposing GET /workers and POST /workers/heartbeat.
    """
    r = APIRouter(prefix="/workers", tags=["workers"])

    @r.get("")
    async def list_(role: Optional[str] = None, limit: int = 100) -> list[dict[str, Any]]:
        """List worker presence rows, optionally filtered by role.

        Args:
            role: Optional role to filter on.
            limit: Maximum number of rows to return.

        Returns:
            The matching worker documents.
        """
        args: dict[str, Any] = {"limit": limit}
        if role is not None:
            args["role"] = role
        return await convex.query("workers:list", args)

    @r.post("/heartbeat")
    async def heartbeat(body: HeartbeatBody) -> Response:
        """Upsert a worker's presence row, stamping lastHeartbeat to now.

        Args:
            body: The worker's identity, role, status, and current item.

        Returns:
            An empty 204 Response.
        """
        await convex.mutation("workers:upsert", body.model_dump(exclude_none=True))
        return Response(status_code=204)

    return r
