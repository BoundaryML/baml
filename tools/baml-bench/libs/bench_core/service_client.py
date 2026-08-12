"""Typed async HTTP client for the baml-bench service.

Every processor and the ingress gateway talk to the service through
this — they never import the Convex client. The service is the only
Convex gateway.
"""

from __future__ import annotations

import json
from typing import Any, AsyncIterator, Optional

import httpx

# Default claim wiring: most tables are claimed on `status` via the
# by_status_created index. Issues override this (notion vs fix queues).
DEFAULT_INDEX = "by_status_created"


class ServiceClient:
    """Async HTTP client for the baml-bench service's CRUD, queue, and blob endpoints."""

    def __init__(self, base_url: str, token: str, timeout: float = 30.0):
        """Open a bearer-authenticated httpx client against the service base URL.

        Args:
            base_url: Root URL of the service; any trailing slash is stripped.
            token: Bearer token sent on every request via the Authorization header.
            timeout: Default per-request timeout in seconds.
        """
        self.base_url = base_url.rstrip("/")
        self._headers = {"Authorization": f"Bearer {token}"}
        self._client = httpx.AsyncClient(
            base_url=self.base_url, headers=self._headers, timeout=timeout
        )

    async def aclose(self) -> None:
        """Close the underlying httpx client and release its connections."""
        await self._client.aclose()

    # ---------- CRUD ----------

    async def create(self, table: str, doc: dict[str, Any]) -> str:
        """Insert a document into a table via the service's POST /{table} endpoint.

        Args:
            table: Target table name (e.g. "tasks").
            doc: Domain fields for the new row; queue defaults (status, attempts,
                timestamps) are filled in server-side.

        Returns:
            The Convex-assigned document id.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.post(f"/{table}", json=doc)
        r.raise_for_status()
        return r.json()["id"]

    async def get(self, table: str, id: str) -> Optional[dict[str, Any]]:
        """Fetch a single document by id.

        Args:
            table: Table the document lives in.
            id: Document id to fetch.

        Returns:
            The document, or None if the service responds 404.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx, non-404 response.
        """
        r = await self._client.get(f"/{table}/{id}")
        if r.status_code == 404:
            return None
        r.raise_for_status()
        return r.json()

    async def list(self, table: str, **query: Any) -> list[dict[str, Any]]:
        """List documents in a table, filtered by the supplied query params.

        Args:
            table: Table to list.
            **query: Filter parameters forwarded as query string args; entries
                whose value is None are dropped.

        Returns:
            The matching documents.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        params = {k: v for k, v in query.items() if v is not None}
        r = await self._client.get(f"/{table}", params=params)
        r.raise_for_status()
        return r.json()

    async def update(self, table: str, id: str, patch: dict[str, Any]) -> dict[str, Any]:
        """Apply a partial update to a document.

        Args:
            table: Table the document lives in.
            id: Document id to patch.
            patch: Fields to merge into the existing document.

        Returns:
            The updated document.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.patch(f"/{table}/{id}", json=patch)
        r.raise_for_status()
        return r.json()

    async def remove(self, table: str, id: str) -> None:
        """Delete a document by id.

        Args:
            table: Table the document lives in.
            id: Document id to delete.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.delete(f"/{table}/{id}")
        r.raise_for_status()

    # ---------- queue verbs ----------

    async def claim(
        self,
        table: str,
        *,
        worker_id: str,
        lease_ms: int,
        value: str,
        claimed_value: str,
        field: str = "status",
        index: str = DEFAULT_INDEX,
    ) -> Optional[dict[str, Any]]:
        """Atomically claim one queued document, flipping its field and stamping a lease.

        Selects one document where `field == value`, flips that field to
        `claimed_value`, and stamps a worker/lease so other workers skip it
        until the lease expires.

        Args:
            table: Table to claim from.
            worker_id: Identifier of the claiming worker, recorded on the lease.
            lease_ms: Lease duration in milliseconds before the claim expires.
            value: Field value that marks a document as claimable.
            claimed_value: Value the field is flipped to once claimed.
            field: Document field the claim reads and flips.
            index: Convex index used to scan for claimable documents.

        Returns:
            The claimed document, or None when the queue is empty.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        body = {
            "workerId": worker_id,
            "leaseMs": lease_ms,
            "field": field,
            "value": value,
            "claimedValue": claimed_value,
            "index": index,
        }
        r = await self._client.post(f"/{table}/claim", json=body)
        r.raise_for_status()
        data = r.json()
        return data if data else None

    async def transition(
        self,
        table: str,
        id: str,
        to: str,
        *,
        field: str = "status",
        patch: Optional[dict[str, Any]] = None,
        release_claim: bool = True,
    ) -> dict[str, Any]:
        """Transition a claimed document's field to a new value and release its lease.

        Should transition on the same field that was claimed so an unrelated
        lifecycle field is not corrupted.

        Args:
            table: Table the document lives in.
            id: Document id to transition.
            to: New value to set on the field.
            field: Field to transition; defaults to the claimed status field.
            patch: Optional additional fields to merge during the transition.
            release_claim: Whether to clear the worker/lease; pass False to keep
                the claim held across the transition.

        Returns:
            The updated document.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        body = {"to": to, "field": field, "patch": patch or {}, "releaseClaim": release_claim}
        r = await self._client.post(f"/{table}/{id}/transition", json=body)
        r.raise_for_status()
        return r.json()

    async def heartbeat(self, table: str, id: str, lease_ms: int) -> None:
        """Extend a claimed document's lease so a long-running job keeps its claim.

        Args:
            table: Table the document lives in.
            id: Document id whose lease to extend.
            lease_ms: New lease duration in milliseconds from now.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.post(f"/{table}/{id}/heartbeat", json={"leaseMs": lease_ms})
        r.raise_for_status()

    # ---------- SSE wake stream ----------

    async def events(
        self, table: str, *, value: str, field: str = "status", index: str = DEFAULT_INDEX
    ) -> AsyncIterator[int]:
        """Stream the claimable-document count over SSE, yielding on each change.

        Opens a long-lived GET /{table}/events stream with the read timeout
        disabled and parses each `data:` line for the current count.

        Args:
            table: Table to watch for claimable documents.
            value: Field value that marks a document as claimable.
            field: Document field the count is keyed on.
            index: Convex index used to count claimable documents.

        Yields:
            The current count of claimable documents whenever it changes.

        Raises:
            httpx.HTTPStatusError: If the stream response is non-2xx.
        """
        params = {"field": field, "value": value, "index": index}
        # httpx: timeout=None means "client default"; to truly disable the
        # read timeout on a long-lived SSE stream use an explicit Timeout.
        stream_timeout = httpx.Timeout(None, connect=15.0)
        async with self._client.stream(
            "GET", f"/{table}/events", params=params, timeout=stream_timeout
        ) as resp:
            resp.raise_for_status()
            async for line in resp.aiter_lines():
                if line.startswith("data:"):
                    payload = line[5:].strip()
                    try:
                        yield int(json.loads(payload)["count"])
                    except (json.JSONDecodeError, KeyError, ValueError):
                        continue

    # ---------- transcript blobs ----------

    async def put_transcript(self, table: str, id: str, text: str) -> str:
        """Upload a transcript blob for a document and return its storage id.

        Args:
            table: Table the document lives in.
            id: Document id the transcript belongs to.
            text: Transcript body; uploaded UTF-8 encoded.

        Returns:
            The storage id assigned to the uploaded blob.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.post(
            f"/{table}/{id}/transcript", content=text.encode(), timeout=120.0
        )
        r.raise_for_status()
        return r.json()["storageId"]

    async def get_transcript(self, storage_id: str) -> str:
        """Fetch the text of a previously uploaded transcript blob.

        Args:
            storage_id: Storage id returned by put_transcript.

        Returns:
            The transcript body as text.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.get(f"/transcripts/{storage_id}", timeout=120.0)
        r.raise_for_status()
        return r.text

    # ---------- baml version ----------

    async def baml_current(self) -> Optional[dict[str, Any]]:
        """Fetch the currently pinned baml build.

        Returns:
            The current baml build record, or None if the service responds 404.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx, non-404 response.
        """
        r = await self._client.get("/baml/current")
        if r.status_code == 404:
            return None
        r.raise_for_status()
        return r.json()

    async def baml_update(self) -> dict[str, Any]:
        """Trigger the service to refresh the pinned baml build.

        Returns:
            The service's response describing the refreshed build.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.post("/baml/update")
        r.raise_for_status()
        return r.json()

    async def put_baml_binary(self, build_id: str, data: bytes) -> dict[str, Any]:
        """Upload the compiled baml CLI binary for a build.

        Args:
            build_id: Identifier of the baml build the binary belongs to.
            data: Raw bytes of the compiled CLI binary.

        Returns:
            The service's response describing the stored binary.

        Raises:
            httpx.HTTPStatusError: If the service returns a non-2xx response.
        """
        r = await self._client.post(
            f"/baml-builds/{build_id}/binary", content=data, timeout=300.0
        )
        r.raise_for_status()
        return r.json()
