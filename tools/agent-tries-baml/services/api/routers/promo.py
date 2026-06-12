"""Promo-code endpoints (the t-shirts bot's inventory, now on Convex).

promoCodes is NOT a claimable queue — issuing a code is one synchronous OCC
mutation — so it gets this small dedicated router instead of the generic
queue router in table.py (whose claim/transition verbs don't exist for it).
"""

from __future__ import annotations

from typing import Any, Optional

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..convex_gateway import ConvexGateway


class ClaimBody(BaseModel):
    """Request body for claiming the next unused promo code."""

    claimedBy: str
    claimedByUserId: str
    notes: Optional[str] = None


def make_promo_router(convex: ConvexGateway) -> APIRouter:
    """Build the promo-code router.

    Args:
        convex: Gateway used to invoke the promoCodes Convex functions.

    Returns:
        An APIRouter exposing promoCodes CRUD plus POST /promo/claim.
    """
    r = APIRouter(tags=["promo"])

    @r.post("/promo/claim")
    async def claim(body: ClaimBody) -> dict[str, Any]:
        """Atomically claim the next unused code (lowest position).

        Args:
            body: Who is claiming and the audit note.

        Returns:
            ``{"code": <code>}``, with code null when inventory is exhausted.
        """
        code = await convex.mutation("promoCodes:claimNext", body.model_dump())
        return {"code": code}

    @r.post("/promoCodes")
    async def create(doc: dict[str, Any]) -> dict[str, str]:
        """Insert a promo code row (used by the one-off SQLite migration).

        Args:
            doc: Field values for the new row.

        Returns:
            A dict with the new document id under ``id``.
        """
        new_id = await convex.mutation("promoCodes:create", {"doc": doc})
        return {"id": new_id}

    @r.get("/promoCodes")
    async def list_(field: Optional[str] = None, value: Optional[str] = None,
                    index: Optional[str] = None, limit: int = 100) -> list[dict[str, Any]]:
        """List promo codes, optionally filtered by an indexed field/value.

        Args:
            field: Optional field to filter on (e.g. "status" or "code").
            value: Optional value the field must equal.
            index: Optional Convex index to query.
            limit: Maximum number of rows to return.

        Returns:
            The matching code documents.
        """
        args: dict[str, Any] = {"limit": limit}
        if field is not None:
            args["field"] = field
        if value is not None:
            args["value"] = value
        if index is not None:
            args["index"] = index
        return await convex.query("promoCodes:list", args)

    @r.patch("/promoCodes/{item_id}")
    async def update(item_id: str, patch: dict[str, Any]) -> dict[str, Any]:
        """Apply a partial update to a promo code row (ops: restore/void codes).

        Args:
            item_id: Convex document id of the row.
            patch: Fields to merge into the row.

        Returns:
            The updated row document.
        """
        return await convex.mutation("promoCodes:update", {"id": item_id, "patch": patch})

    @r.get("/promoCodes/{item_id}")
    async def get(item_id: str) -> dict[str, Any]:
        """Fetch a single promo code row by id.

        Args:
            item_id: Convex document id of the row.

        Returns:
            The row document.

        Raises:
            HTTPException: 404 when no row has that id.
        """
        doc = await convex.query("promoCodes:get", {"id": item_id})
        if doc is None:
            raise HTTPException(404, "not found")
        return doc

    return r
