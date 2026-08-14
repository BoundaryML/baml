"""baml-builder processor: claim a queued build, download the alpha-channel
release binary for this platform, upload it, mark ready."""

from __future__ import annotations

import logging
import time
from typing import Any

from bench_core.processor import Processor, run_processor

from . import build

log = logging.getLogger("baml_builder")


class BamlBuilder(Processor):
    """Processor that fetches a queued BAML alpha build and marks it ready."""

    role = "baml-builder"
    table = "bamlBuilds"
    claim_value = "queued"
    claim_into = "building"
    lease_ms = 10 * 60 * 1000  # just a download now
    heartbeat_secs = 30.0

    async def process(self, item: dict[str, Any]) -> None:
        """Download the alpha release binary for a claimed build and store it.

        Fetches the platform-specific binary for the build's release tag,
        uploads it, and transitions the build to "ready"; on failure the build
        is transitioned to "failed" with the error tail recorded.

        Args:
            item: The claimed build record, including its `_id`, release `ref`
                tag, and `sha`.

        Returns:
            None.
        """
        build_id = item["_id"]
        tag = item.get("ref")  # the alpha release tag, e.g. baml-language-0.11.0-alpha.4166
        log.info("fetching baml alpha release %s (sha=%s)", tag, item.get("sha"))
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


if __name__ == "__main__":
    run_processor(BamlBuilder)
