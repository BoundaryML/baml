"""In-memory stand-in for ConvexGateway — same interface, no backend.

Reproduces the table-agnostic CRUD + claimable-queue semantics that the real
Convex functions implement in ``convex/lib.ts`` (see that file for the contract):

  * create   - insert with default attempts/timestamps, null keys stripped
  * get      - by id, or None
  * list     - newest-first, optionally filtered to field==value, capped at limit
  * update   - patch + bump updatedAt, null keys stripped
  * remove   - delete by id
  * claim    - flip the OLDEST field==value row to claimedValue, stamp owner+lease,
               bump attempts (atomic: no await inside, so one winner per claim)
  * transition - set field=to (+patch), optionally clear the claim/lease
  * heartbeat  - push the lease out
  * countClaimable - number of field==value rows (capped at 1000)

Selected via ``CONVEX_BACKEND=memory`` (see ``gateway_from_env``). It lets the
whole stack run locally and in tests with no Convex deployment and no Docker.
It is NOT a persistence layer (state lives only in this process) and does not
reproduce Convex's real indexes/OCC — ``claim`` is atomic only because it never
awaits. For end-to-end fidelity against the real backend, use the Convex path.
"""

from __future__ import annotations

import asyncio
import copy
import time
import uuid
from typing import Any, AsyncIterator, Optional


def _strip_nulls(d: dict[str, Any]) -> dict[str, Any]:
    """Drop keys whose value is None, mirroring the backend's stripNulls.

    Convex treats ``v.optional(T)`` as "absent or T" (a literal null is invalid),
    and the Python clients serialize None -> null, so a None means "field absent".

    Args:
        d: The dict to filter.

    Returns:
        A new dict without the None-valued keys.
    """
    return {k: v for k, v in d.items() if v is not None}


class MemoryGateway:
    """Async in-memory gateway with the ConvexGateway query/mutation/action API."""

    def __init__(self, poll_interval: float = 0.1):
        """Initialize an empty store.

        Args:
            poll_interval: Seconds between polls in subscribe_counts.
        """
        self._tables: dict[str, dict[str, dict[str, Any]]] = {}
        self._poll = poll_interval
        # Strictly increasing stamp for _creationTime + claim/list ordering, so
        # "oldest"/"newest" are unambiguous even within the same millisecond.
        self._ct = float(int(time.time() * 1000))

    # ---------- helpers ----------
    @staticmethod
    def _now() -> int:
        """Return the current time in milliseconds (for createdAt/lease fields)."""
        return int(time.time() * 1000)

    def _tbl(self, table: str) -> dict[str, dict[str, Any]]:
        """Return the (lazily created) row map for a table.

        Args:
            table: Table name.

        Returns:
            The id -> document map for the table.
        """
        return self._tables.setdefault(table, {})

    # ---------- mutations ----------
    def _create(self, table: str, doc: dict[str, Any]) -> str:
        """Insert a row with default attempts/timestamps; return its id."""
        self._ct += 1
        now = self._now()
        full = _strip_nulls({"attempts": 0, "createdAt": now, "updatedAt": now, **doc})
        full["_id"] = uuid.uuid4().hex
        full["_creationTime"] = self._ct
        self._tbl(table)[full["_id"]] = full
        return full["_id"]

    def _update(self, table: str, id: str, patch: dict[str, Any]) -> Optional[dict[str, Any]]:
        """Merge a patch (null keys stripped) and bump updatedAt; return the row."""
        doc = self._tbl(table).get(id)
        if doc is None:
            return None
        doc.update(_strip_nulls(patch))
        doc["updatedAt"] = self._now()
        return copy.deepcopy(doc)

    def _remove(self, table: str, id: str) -> None:
        """Delete a row by id (no-op if absent)."""
        self._tbl(table).pop(id, None)

    def _claim(self, table: str, *, field: str, value: str, claimed_value: str,
               worker_id: str, lease_ms: int) -> Optional[dict[str, Any]]:
        """Claim the oldest field==value row: flip it, stamp owner + lease, bump attempts.

        Atomic by construction — there is no ``await`` between the scan and the
        write, so concurrent claims on the same event loop each pick a distinct row.
        """
        rows = [r for r in self._tbl(table).values() if r.get(field) == value]
        if not rows:
            return None
        doc = min(rows, key=lambda r: r["_creationTime"])  # oldest claimable
        now = self._now()
        doc[field] = claimed_value
        doc["claimedBy"] = worker_id
        doc["claimedAt"] = now
        doc["leaseExpiresAt"] = now + lease_ms
        doc["attempts"] = (doc.get("attempts") or 0) + 1
        doc["updatedAt"] = now
        return copy.deepcopy(doc)

    def _transition(self, table: str, id: str, to: str, *, field: str = "status",
                    patch: Optional[dict[str, Any]] = None,
                    release_claim: bool = True) -> Optional[dict[str, Any]]:
        """Set field=to (then apply patch), optionally clearing the claim/lease."""
        doc = self._tbl(table).get(id)
        if doc is None:
            return None
        doc[field] = to
        doc["updatedAt"] = self._now()
        doc.update(_strip_nulls(patch or {}))  # patch wins, mirroring lib.ts spread order
        if release_claim is not False:
            for k in ("claimedBy", "claimedAt", "leaseExpiresAt"):
                doc.pop(k, None)
        return copy.deepcopy(doc)

    def _heartbeat(self, table: str, id: str, lease_ms: int) -> None:
        """Push a claimed row's lease out by lease_ms from now."""
        doc = self._tbl(table).get(id)
        if doc is not None:
            now = self._now()
            doc["leaseExpiresAt"] = now + lease_ms
            doc["updatedAt"] = now

    # ---------- queries ----------
    def _get(self, table: str, id: str) -> Optional[dict[str, Any]]:
        """Return a deep copy of the row, or None."""
        doc = self._tbl(table).get(id)
        return copy.deepcopy(doc) if doc is not None else None

    def _list(self, table: str, field: Optional[str], value: Optional[str],
              limit: int) -> list[dict[str, Any]]:
        """Return up to limit rows newest-first, optionally filtered to field==value."""
        rows = list(self._tbl(table).values())
        if field is not None and value is not None:
            rows = [r for r in rows if r.get(field) == value]
        rows.sort(key=lambda r: r["_creationTime"], reverse=True)  # newest first
        return [copy.deepcopy(r) for r in rows[:limit]]

    def _count(self, table: str, field: str, value: str) -> int:
        """Count field==value rows, capped at 1000 like the backend."""
        return min(1000, sum(1 for r in self._tbl(table).values() if r.get(field) == value))

    # ---------- public API (mirrors ConvexGateway) ----------
    async def mutation(self, name: str, args: dict[str, Any]) -> Any:
        """Dispatch a ``{table}:{verb}`` mutation against the in-memory store.

        Args:
            name: Function path, e.g. ``tasks:claim``.
            args: The mutation's argument object.

        Returns:
            The mutation's value (new id, row, or None).

        Raises:
            ValueError: For an unknown mutation verb.
        """
        # Custom named mutations (not the generic queue verbs) get explicit
        # stand-ins mirroring their convex/*.ts implementations.
        if name == "promoCodes:claimNext":
            rows = [r for r in self._tbl("promoCodes").values() if r.get("status") == "unused"]
            if not rows:
                return None
            doc = min(rows, key=lambda r: r.get("position", 0))
            now = self._now()
            doc["status"] = "used"
            doc["claimedBy"] = args["claimedBy"]
            doc["claimedByUserId"] = args["claimedByUserId"]
            if args.get("notes") is not None:
                doc["notes"] = args["notes"]
            doc["claimedAt"] = now
            doc["updatedAt"] = now
            return doc["code"]
        if name == "workers:upsert":
            now = self._now()
            existing = next(
                (r for r in self._tbl("workers").values() if r.get("workerId") == args["workerId"]),
                None,
            )
            fields = {
                "role": args["role"],
                "status": args["status"],
                "lastHeartbeat": now,
            }
            if args["status"] == "busy" and args.get("currentItemId") is not None:
                fields["currentItemId"] = args["currentItemId"]
            if existing is not None:
                if args["status"] != "busy":
                    existing.pop("currentItemId", None)
                existing.update(fields)
                return existing["_id"]
            return self._create("workers", {"workerId": args["workerId"], **fields})
        table, verb = name.split(":", 1)
        if verb == "create":
            return self._create(table, args["doc"])
        if verb == "update":
            return self._update(table, args["id"], args.get("patch") or {})
        if verb == "remove":
            return self._remove(table, args["id"])
        if verb == "claim":
            return self._claim(table, field=args.get("field", "status"), value=args["value"],
                               claimed_value=args["claimedValue"], worker_id=args["workerId"],
                               lease_ms=args["leaseMs"])
        if verb == "transition":
            return self._transition(table, args["id"], args["to"],
                                    field=args.get("field", "status"),
                                    patch=args.get("patch") or {},
                                    release_claim=args.get("releaseClaim", True))
        if verb == "heartbeat":
            return self._heartbeat(table, args["id"], args["leaseMs"])
        raise ValueError(f"unknown mutation: {name}")

    async def query(self, name: str, args: dict[str, Any]) -> Any:
        """Dispatch a ``{table}:{verb}`` query against the in-memory store.

        Args:
            name: Function path, e.g. ``trophies:list``.
            args: The query's argument object.

        Returns:
            The query's value (row, list, or count).

        Raises:
            ValueError: For an unknown query verb.
        """
        # workers:list filters by role (not the generic field/value contract).
        if name == "workers:list":
            rows = list(self._tbl("workers").values())
            if args.get("role"):
                rows = [r for r in rows if r.get("role") == args["role"]]
            rows.sort(key=lambda r: r["_creationTime"], reverse=True)
            return [copy.deepcopy(r) for r in rows[: args.get("limit", 100)]]
        table, verb = name.split(":", 1)
        if verb == "get":
            return self._get(table, args["id"])
        if verb == "list":
            return self._list(table, args.get("field"), args.get("value"), args.get("limit", 100))
        if verb == "countClaimable":
            return self._count(table, args["field"], args["value"])
        raise ValueError(f"unknown query: {name}")

    async def action(self, name: str, args: dict[str, Any]) -> Any:
        """Actions are unsupported by the in-memory backend (none are used).

        Args:
            name: Function path.
            args: The action's argument object.

        Raises:
            NotImplementedError: Always.
        """
        raise NotImplementedError(f"MemoryGateway has no actions: {name}")

    async def subscribe_counts(self, name: str, args: dict[str, Any]) -> AsyncIterator[int]:
        """Poll the claimable count and yield whenever it changes.

        Args:
            name: Count query path, e.g. ``tasks:countClaimable``.
            args: Argument object carrying ``field`` and ``value``.

        Yields:
            The latest count, emitted only when it differs from the previous value.
        """
        table, _ = name.split(":", 1)
        last: Optional[int] = None
        while True:
            count = self._count(table, args["field"], args["value"])
            if count != last:
                last = count
                yield count
            await asyncio.sleep(self._poll)

    async def aclose(self) -> None:
        """No-op close, for parity with clients that hold resources."""
        return None
