"""Unit coverage for the Processor claim loop (``libs/bench_core/processor.py``).

Drives a tiny Processor subclass through ``_drain()`` with an in-memory fake service
(modeled on the ``FakeService`` in ``test_ingress_routing.py``) so the core queue
semantics are tested with no backend, no Docker, and no secrets: claim-until-empty,
one ``process()`` per item, batch mode stopping after one, and the on-exception path
that records ``lastError`` and fails the item on the claim field (not ``status``).
"""

from __future__ import annotations

import asyncio

from bench_core.processor import Processor


class FakeQueueService:
    """In-memory stand-in for the ServiceClient verbs the Processor calls."""

    def __init__(self, items):
        """Initialize with the docs to hand out, one per ``claim`` call, in order.

        Args:
            items: Documents returned by successive ``claim`` calls (then None).
        """
        self._items = list(items)
        self.transitions: list[tuple] = []   # (id, to, field)
        self.updates: list[tuple] = []        # (id, patch)
        self.heartbeats: list[tuple] = []     # (id, lease_ms)

    async def claim(self, table, *, worker_id, lease_ms, field, value, claimed_value, index):
        """Hand out the next queued doc, or None when the queue is drained.

        Args:
            table: Table name (ignored).
            worker_id: Claiming worker id (ignored).
            lease_ms: Lease duration (ignored).
            field: Claim field (ignored).
            value: Claimable value (ignored).
            claimed_value: In-flight value (ignored).
            index: Claim index (ignored).

        Returns:
            The next document, or None when empty.
        """
        return self._items.pop(0) if self._items else None

    async def transition(self, table, id, to, *, field="status", patch=None, release_claim=True):
        """Record a transition call.

        Args:
            table: Table name (ignored).
            id: Document id; recorded.
            to: Target value; recorded.
            field: Field transitioned; recorded.
            patch: Optional patch (ignored).
            release_claim: Whether to release the claim (ignored).

        Returns:
            An empty dict.
        """
        self.transitions.append((id, to, field))
        return {}

    async def update(self, table, id, patch):
        """Record an update call.

        Args:
            table: Table name (ignored).
            id: Document id; recorded.
            patch: The patch dict; recorded.

        Returns:
            An empty dict.
        """
        self.updates.append((id, patch))
        return {}

    async def heartbeat(self, table, id, lease_ms):
        """Record a heartbeat call.

        Args:
            table: Table name (ignored).
            id: Document id; recorded.
            lease_ms: Lease duration; recorded.
        """
        self.heartbeats.append((id, lease_ms))


class _RecordingProcessor(Processor):
    """Processor that just records the ids it processes."""

    role = "test-rec"
    table = "tasks"
    claim_value = "queued"
    claim_into = "running"

    def __init__(self, service):
        """Bind the fake service and an empty seen-list.

        Args:
            service: The fake queue service.
        """
        super().__init__(service)
        self.seen: list[str] = []

    async def process(self, item):
        """Record the item id.

        Args:
            item: The claimed document.
        """
        self.seen.append(item["_id"])


class _FailingProcessor(Processor):
    """Processor that claims on a non-status field and always raises in process()."""

    role = "test-fail"
    table = "issues"
    claim_field = "notionSyncStatus"
    claim_value = "dirty"
    claim_into = "syncing"

    async def process(self, item):
        """Always fail, to exercise the error path.

        Args:
            item: The claimed document.

        Raises:
            RuntimeError: Always.
        """
        raise RuntimeError("boom")


async def test_drain_claims_until_empty():
    """_drain claims and processes each item, then stops when claim returns None."""
    svc = FakeQueueService([{"_id": "a"}, {"_id": "b"}])
    proc = _RecordingProcessor(svc)
    await proc._drain()
    await asyncio.sleep(0)  # let the per-item heartbeat tasks finish cancelling
    assert proc.seen == ["a", "b"]


async def test_batch_processes_one_per_drain():
    """In batch mode, _drain returns after a single item even with more queued."""
    class _BatchProc(_RecordingProcessor):
        batch = True

    svc = FakeQueueService([{"_id": "a"}, {"_id": "b"}])
    proc = _BatchProc(svc)
    await proc._drain()
    await asyncio.sleep(0)
    assert proc.seen == ["a"]


async def test_run_one_fails_on_claim_field():
    """A process() exception records lastError and fails the item on the claim field."""
    svc = FakeQueueService([{"_id": "x"}])
    proc = _FailingProcessor(svc)
    await proc._drain()
    await asyncio.sleep(0)
    assert svc.updates and svc.updates[0][0] == "x"
    assert "boom" in svc.updates[0][1]["lastError"]
    # failed on the claim field (notionSyncStatus), NOT the lifecycle "status"
    assert ("x", "failed", "notionSyncStatus") in svc.transitions
