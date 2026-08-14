"""One router factory → identical CRUD + queue verbs for every table.

This is where the symmetry lives: tasks, trophies, issues and bamlBuilds
all get the same endpoint set from `make_router(table)`.
"""

from __future__ import annotations

from typing import Any, Optional

from fastapi import APIRouter, HTTPException, Request, Response
from pydantic import BaseModel
from sse_starlette.sse import EventSourceResponse

from ..convex_gateway import ConvexGateway
from .. import blobs


class ClaimBody(BaseModel):
    """Request body for claiming the next item off a table's queue."""

    workerId: str
    leaseMs: int
    field: str = "status"
    value: str
    claimedValue: str
    index: str = "by_status_created"


class TransitionBody(BaseModel):
    """Request body for transitioning an item to a new status."""

    to: str
    field: str = "status"
    patch: dict[str, Any] = {}
    releaseClaim: bool = True


class HeartbeatBody(BaseModel):
    """Request body for extending a claimed item's lease."""

    leaseMs: int


def make_router(table: str, convex: ConvexGateway) -> APIRouter:
    """Build a CRUD + queue + SSE router for a single Convex table.

    Every table (tasks, trophies, issues, bamlBuilds) gets the same endpoint
    set, each delegating to the table's matching Convex functions.

    Args:
        table: Convex table name; also the route prefix and function namespace.
        convex: Gateway used to invoke the table's Convex functions.

    Returns:
        An APIRouter exposing the table's endpoints.
    """
    r = APIRouter(prefix=f"/{table}", tags=[table])

    @r.post("")
    async def create(doc: dict[str, Any]) -> dict[str, str]:
        """Insert a new row into the table.

        Args:
            doc: Domain fields for the new row.

        Returns:
            A dict with the new document id under ``id``.
        """
        new_id = await convex.mutation(f"{table}:create", {"doc": doc})
        return {"id": new_id}

    @r.get("/events")
    async def events(request: Request, value: str, field: str = "status",
                     index: str = "by_status_created") -> EventSourceResponse:
        """Stream claimable-count changes for the table as SSE.

        Args:
            request: Incoming request, used to detect client disconnects.
            value: Field value defining the claimable set (e.g. ``queued``).
            field: Field the count is keyed on.
            index: Convex index used by the count query.

        Returns:
            An EventSourceResponse emitting ``{"count": N}`` on each change.
        """
        async def stream():
            """Yield SSE payloads as the claimable count changes."""
            async for count in convex.subscribe_counts(
                f"{table}:countClaimable", {"field": field, "value": value, "index": index}
            ):
                if await request.is_disconnected():
                    break
                yield {"data": f'{{"count": {count}}}'}

        return EventSourceResponse(stream())

    @r.get("/{item_id}")
    async def get(item_id: str) -> dict[str, Any]:
        """Fetch a single row by id.

        Args:
            item_id: Convex document id of the row.

        Returns:
            The row document.

        Raises:
            HTTPException: 404 when no row has that id.
        """
        doc = await convex.query(f"{table}:get", {"id": item_id})
        if doc is None:
            raise HTTPException(404, "not found")
        return doc

    @r.get("")
    async def list_(field: Optional[str] = None, value: Optional[str] = None,
                    index: Optional[str] = None, limit: int = 100) -> list[dict[str, Any]]:
        """List rows, optionally filtered by an indexed field/value.

        Args:
            field: Optional field to filter on.
            value: Optional value the field must equal.
            index: Optional Convex index to query.
            limit: Maximum number of rows to return.

        Returns:
            The matching row documents.
        """
        args: dict[str, Any] = {"limit": limit}
        if field is not None:
            args["field"] = field
        if value is not None:
            args["value"] = value
        if index is not None:
            args["index"] = index
        return await convex.query(f"{table}:list", args)

    @r.patch("/{item_id}")
    async def update(item_id: str, patch: dict[str, Any]) -> dict[str, Any]:
        """Apply a partial update to a row.

        Args:
            item_id: Convex document id of the row.
            patch: Fields to merge into the row.

        Returns:
            The updated row document.
        """
        return await convex.mutation(f"{table}:update", {"id": item_id, "patch": patch})

    @r.delete("/{item_id}")
    async def remove(item_id: str) -> Response:
        """Delete a row by id.

        Args:
            item_id: Convex document id of the row.

        Returns:
            An empty 204 Response.
        """
        await convex.mutation(f"{table}:remove", {"id": item_id})
        return Response(status_code=204)

    @r.post("/claim")
    async def claim(body: ClaimBody) -> Any:
        """Atomically claim the next queued item for a worker.

        Args:
            body: Claim parameters (worker id, lease, field/value, index).

        Returns:
            The claimed row document, or null when the queue is empty.
        """
        doc = await convex.mutation(f"{table}:claim", body.model_dump())
        return doc  # null when queue empty

    @r.post("/{item_id}/transition")
    async def transition(item_id: str, body: TransitionBody) -> dict[str, Any]:
        """Transition a claimed item to a new status.

        Args:
            item_id: Convex document id of the row.
            body: Transition parameters (target status, field, patch, whether
                to release the claim).

        Returns:
            The updated row document.
        """
        return await convex.mutation(
            f"{table}:transition",
            {"id": item_id, "to": body.to, "field": body.field,
             "patch": body.patch, "releaseClaim": body.releaseClaim},
        )

    @r.post("/{item_id}/heartbeat")
    async def heartbeat(item_id: str, body: HeartbeatBody) -> Response:
        """Extend the lease on a claimed item.

        Args:
            item_id: Convex document id of the claimed row.
            body: Heartbeat parameters carrying the new lease duration.

        Returns:
            An empty 204 Response.
        """
        await convex.mutation(f"{table}:heartbeat", {"id": item_id, "leaseMs": body.leaseMs})
        return Response(status_code=204)

    @r.post("/{item_id}/transcript")
    async def put_transcript(item_id: str, request: Request) -> dict[str, str]:
        """Store a row's transcript blob and record its pointer on the row.

        Args:
            item_id: Convex document id of the row.
            request: Request whose raw body is the transcript text.

        Returns:
            A dict with the blob storage id under ``storageId``.
        """
        text = (await request.body()).decode()
        storage_id = blobs.put_text(table, item_id, text)
        await convex.mutation(
            f"{table}:update", {"id": item_id, "patch": {"transcriptStorageId": storage_id}}
        )
        return {"storageId": storage_id}

    return r
