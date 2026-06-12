"""Changelog worker: claims queued changelogEntries and generates them.

queued -> generating -> done | failed. A revise/regenerate request is just a
done row flipped back to queued with reviseMode/reviseGuidance set — the
claim is the per-version lock (replacing the old service's in-process
semaphore + inflight set). When the row carries slack routing (a @bammy
edit), the completion or failure is posted back to that thread.
"""

from __future__ import annotations

import logging
import os
from typing import Any, Optional

from bench_core import changelog_github, slack_client
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient

from .generation import GenerationError, generate

log = logging.getLogger("changelog_worker")

# Public changelog page, linked in the Slack success reply.
CHANGELOG_PUBLIC_URL = os.environ.get(
    "CHANGELOG_PUBLIC_URL", "https://new.boundaryml.com/changelog"
)


class ChangelogWorker(Processor):
    """Claim queued changelog entries and run the draft/critique generation."""

    role = "changelog-worker"
    table = "changelogEntries"
    claim_value = "queued"
    claim_into = "generating"
    lease_ms = 30 * 60 * 1000  # generation worst-cases around 15 min

    def __init__(self, service):
        """Initialize the processor with the claude-proxy client.

        Args:
            service: The ServiceClient for all claim/transition calls.
        """
        super().__init__(service)
        self.proxy = ProxyClient.from_env()

    async def process(self, item: dict[str, Any]) -> None:
        """Generate one entry and write it back onto the claimed row.

        Args:
            item: The claimed changelogEntries row.

        Raises:
            GenerationError: Propagated so the base loop marks the row failed
                (after the Slack failure reply is posted).
        """
        version = item.get("version") or "?"
        tag = item.get("tag") or changelog_github.to_tag(version, item.get("channel"))
        mode = (item.get("reviseMode") or "").strip()
        guidance = (item.get("reviseGuidance") or "").strip()

        revise_seed: Optional[dict[str, Any]] = None
        if mode == "revise" and item.get("body"):
            revise_seed = {
                "current_entry": {
                    k: item.get(k) for k in ("version", "date", "title", "body", "authors", "channel")
                },
                "guidance": guidance or "Improve this entry; keep it grounded in the diff.",
            }

        try:
            entry = await generate(self.proxy, tag, item.get("fromRelease"), revise_seed)
        except GenerationError as e:
            await self._reply(item, f"Changelog generation for `{version}` failed: {e}")
            raise

        await self.service.transition(self.table, item["_id"], "done", patch={
            "tag": tag,
            "date": entry.get("date"),
            "title": entry.get("title"),
            "body": entry.get("body"),
            "authors": entry.get("authors") or [],
            "channel": entry.get("channel") or item.get("channel") or "unknown",
            "meta": entry.get("meta"),
            # Clear the consumed revise inputs ("" rather than None: the
            # service strips null keys, so None would leave stale values).
            "reviseMode": "",
            "reviseGuidance": "",
        })
        log.info("changelog: published %s (attempts=%s verdict=%s)", version,
                 (entry.get("meta") or {}).get("attempts"),
                 (entry.get("meta") or {}).get("final_verdict"))

        verdict = (entry.get("meta") or {}).get("final_verdict", "?")
        await self._reply(
            item,
            f"Done. `{version}` is updated and live.\n"
            f"*{entry.get('title')}*  (verdict: {verdict})\n"
            f"{CHANGELOG_PUBLIC_URL}",
        )

    @staticmethod
    async def _reply(item: dict[str, Any], text: str) -> None:
        """Post a threaded Slack reply when the row carries routing; best-effort.

        Args:
            item: The entry row (slackChannel/slackThreadTs may be set).
            text: The message to post.
        """
        channel = item.get("slackChannel")
        if not channel:
            return
        try:
            await slack_client.post_message(
                os.environ.get("ATB_SLACK_BOT_TOKEN", ""), channel, text,
                thread_ts=item.get("slackThreadTs"),
            )
        except Exception:  # noqa: BLE001
            log.exception("changelog: slack reply failed (ignored)")


if __name__ == "__main__":
    run_processor(ChangelogWorker)
