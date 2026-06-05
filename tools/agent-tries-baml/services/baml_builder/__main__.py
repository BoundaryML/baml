"""baml-builder processor: claim a queued build, download the nightly-channel
release binary for this platform, upload it, mark ready, prune old builds."""

from __future__ import annotations

import logging
import time
from typing import Any

from bench_core.processor import Processor, run_processor

from . import build

log = logging.getLogger("baml_builder")


class BamlBuilder(Processor):
    """Processor that fetches a queued BAML nightly build and marks it ready."""

    role = "baml-builder"
    table = "bamlBuilds"
    claim_value = "queued"
    claim_into = "building"
    lease_ms = 10 * 60 * 1000  # just a download now
    heartbeat_secs = 30.0

    async def process(self, item: dict[str, Any]) -> None:
        """Download the nightly release binary for a claimed build and store it.

        Fetches the platform-specific binary for the build's release tag,
        uploads it, transitions the build to "ready", and prunes old builds; on
        failure the build is transitioned to "failed" with the error tail
        recorded.

        Args:
            item: The claimed build record, including its `_id`, release `ref`
                tag, and `sha`.

        Returns:
            None.
        """
        build_id = item["_id"]
        tag = item.get("ref")  # the nightly tag, e.g. baml-language-0.11.3-nightly.20260605.f
        log.info("fetching baml nightly release %s (sha=%s)", tag, item.get("sha"))
        try:
            binary = await build.fetch_baml(tag)
        except Exception as e:  # noqa: BLE001
            log.exception("fetch failed for %s", tag)
            await self.service.transition(
                self.table, build_id, "failed", patch={"buildLogTail": str(e)[-4000:]}
            )
            return
        await self.service.put_baml_binary(build_id, binary)
        await self.service.transition(
            self.table, build_id, "ready", patch={"builtAt": int(time.time() * 1000)}
        )
        log.info("baml %s ready (%d bytes)", tag, len(binary))
        # Keep the builder bucket bounded now that a new build has landed.
        try:
            pruned = await self.service.baml_prune()
            log.info("baml prune: %s", pruned)
        except Exception:  # noqa: BLE001
            log.exception("baml prune failed (non-fatal)")


if __name__ == "__main__":
    run_processor(BamlBuilder)
