"""The Processor base - one event-driven claim loop reused by every stage.

A subclass declares which table/value it consumes, how it claims (status
or another field), and implements `process(item)`. The base handles: SSE
wake-ups, atomic claim, a heartbeat task during the long `process`, and
transition/fail. A slow poll backstops SSE drops.
"""

from __future__ import annotations

import asyncio
import logging
import os
import socket
import uuid
from typing import Any, Optional

from .service_client import DEFAULT_INDEX, ServiceClient

log = logging.getLogger("processor")


class Processor:
    """Event-driven claim loop: a subclass declares its queue and implements process()."""

    # --- subclass config ---
    role: str = "processor"
    table: str = ""
    claim_value: str = ""          # the claimable value (e.g. "queued")
    claim_into: str = ""           # the in-flight value the claim flips to
    claim_field: str = "status"
    claim_index: str = DEFAULT_INDEX
    lease_ms: int = 30 * 60 * 1000  # 30 min
    heartbeat_secs: float = 60.0
    poll_backstop_secs: float = 30.0
    batch: bool = False            # if True, process() is called once per wake, drains itself

    def __init__(self, service: ServiceClient):
        """Bind the service client and mint a unique worker id for this process.

        Args:
            service: Client used for all claim, transition, and heartbeat calls.
        """
        self.service = service
        self.id = f"{self.role}-{socket.gethostname()}-{os.getpid()}-{uuid.uuid4().hex[:6]}"

    # --- subclass implements this ---
    async def process(self, item: dict[str, Any]) -> None:
        """Handle one claimed item; subclasses implement the stage's work.

        Args:
            item: The claimed document to process.

        Raises:
            NotImplementedError: Always, unless overridden by a subclass.
        """
        raise NotImplementedError

    # --- claim wiring (overridable for issues' notion/fix queues) ---
    async def _claim_one(self) -> Optional[dict[str, Any]]:
        """Claim a single item using the subclass's queue config.

        Override for custom queues (e.g. issues' notion/fix fields).

        Returns:
            The claimed document, or None when the queue is empty.
        """
        return await self.service.claim(
            self.table,
            worker_id=self.id,
            lease_ms=self.lease_ms,
            field=self.claim_field,
            value=self.claim_value,
            claimed_value=self.claim_into,
            index=self.claim_index,
        )

    async def _drain(self) -> None:
        """Claim and run items until the queue is empty, or just one in batch mode."""
        while True:
            item = await self._claim_one()
            if item is None:
                return
            await self._run_one(item)
            if self.batch:
                return

    async def _run_one(self, item: dict[str, Any]) -> None:
        """Run process() for one item under a heartbeat task, failing it on error.

        On exception, records lastError and transitions the item to "failed" on
        the same field it was claimed on so an unrelated lifecycle field stays
        intact; the heartbeat task is always cancelled afterward.

        Args:
            item: The claimed document to run.
        """
        item_id = item["_id"]
        hb = asyncio.create_task(self._heartbeat(item_id))
        try:
            await self.process(item)
        except Exception as e:  # noqa: BLE001
            log.exception("process failed for %s/%s", self.table, item_id)
            try:
                await self.service.update(self.table, item_id, {"lastError": str(e)[:2000]})
                # Fail on the SAME field we claimed (so a notionSyncStatus
                # queue doesn't corrupt the issue's lifecycle `status`).
                await self.service.transition(
                    self.table, item_id, "failed", field=self.claim_field
                )
            except Exception:  # noqa: BLE001
                log.exception("failed to mark %s/%s failed", self.table, item_id)
        finally:
            hb.cancel()

    async def _heartbeat(self, item_id: str) -> None:
        """Periodically extend the item's lease until the task is cancelled.

        Args:
            item_id: Id of the claimed document whose lease to renew.
        """
        try:
            while True:
                await asyncio.sleep(self.heartbeat_secs)
                try:
                    await self.service.heartbeat(self.table, item_id, self.lease_ms)
                except Exception:  # noqa: BLE001
                    # A transient heartbeat failure must not kill the loop, or the
                    # lease would stop being renewed and the reaper would steal the
                    # in-flight item; log and keep renewing on the next tick.
                    log.warning("heartbeat error for %s/%s", self.table, item_id)
        except asyncio.CancelledError:
            pass

    async def _poll_backstop(self) -> None:
        """Periodically drain the queue to backstop dropped SSE wake-ups.

        Runs forever on a fixed interval, swallowing drain errors so the loop
        keeps backstopping.
        """
        while True:
            await asyncio.sleep(self.poll_backstop_secs)
            try:
                await self._drain()
            except Exception:  # noqa: BLE001
                log.exception("poll backstop drain failed")

    async def run(self) -> None:
        """Run the main loop: drain on startup, then drain on each SSE wake-up.

        Starts the poll backstop task, catches up on anything already queued,
        then drains on every SSE notification; on stream drops it waits briefly
        and reconnects while the backstop covers the gap.
        """
        log.info("%s starting: table=%s value=%s", self.id, self.table, self.claim_value)
        backstop = asyncio.create_task(self._poll_backstop())
        try:
            try:
                await self._drain()  # catch up on anything already queued
            except Exception:  # noqa: BLE001
                log.warning("initial drain failed; backstop will retry", exc_info=True)
            while True:
                try:
                    async for _count in self.service.events(
                        self.table,
                        field=self.claim_field,
                        value=self.claim_value,
                        index=self.claim_index,
                    ):
                        await self._drain()
                except Exception:  # noqa: BLE001
                    log.warning("SSE stream dropped; relying on poll backstop, retrying in 5s")
                    await asyncio.sleep(5)
        finally:
            backstop.cancel()


def run_processor(proc_factory) -> None:
    """Run a processor to completion from a synchronous entry point.

    Configures logging, builds a ServiceClient from the SERVICE_URL and
    SERVICE_TOKEN env vars, runs the processor's loop inside asyncio.run, and
    closes the client on exit.

    Args:
        proc_factory: Callable that takes a ServiceClient and returns a
            Processor instance to run.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(
        base_url=os.environ["SERVICE_URL"],
        token=os.environ.get("SERVICE_TOKEN", ""),
    )

    async def _main() -> None:
        """Build the processor, run its loop, and close the client on exit."""
        proc = proc_factory(service)
        try:
            await proc.run()
        finally:
            await service.aclose()

    asyncio.run(_main())
