"""Sync the changelog against GitHub: enqueue entries for missing releases.

One implementation shared by the cron poller (every CHANGELOG_POLL_SECS) and
the on-demand ``POST /entries/update`` ingress endpoint (for when the cron
missed a release or was down). Idempotent by version: a release that already
has an entry row — in any status — is never re-enqueued.
"""

from __future__ import annotations

from typing import Any

import anyio

from . import changelog_github
from .service_client import ServiceClient


async def sync_missing_entries(
    service: ServiceClient,
    *,
    channels: tuple[str, ...] = ("nightly", "canary"),
    limit: int = 100,
) -> list[dict[str, Any]]:
    """Enqueue a queued changelogEntries row for every release without one.

    Lists the repo's recent releases on the given channels (GitHub call runs
    on a thread) and the existing entry versions, then creates ``queued``
    rows for the missing ones, oldest-first. The changelog worker picks them
    up; generation is fire-and-forget.

    Args:
        service: The ServiceClient used to list entries and create rows.
        channels: Release channels to consider.
        limit: How many recent GitHub releases to scan.

    Returns:
        One dict per enqueued release: ``{version, tag, channel, id}``.

    Raises:
        changelog_github.GitHubError: When the release listing fails.
    """
    tags = await anyio.to_thread.run_sync(
        lambda: changelog_github.recent_release_tags(channels, limit)
    )
    rows = await service.list("changelogEntries", limit=1000)
    # Filter out version-less rows: a stray None in the set would match a tag
    # normalize() cannot parse and silently skip enqueueing that release.
    known = {r["version"] for r in rows if r.get("version")}
    enqueued: list[dict[str, Any]] = []
    for tag in reversed(tags):  # oldest-first
        version = changelog_github.normalize(tag)
        if version in known:
            continue
        channel = changelog_github.channel_of(tag) or "unknown"
        eid = await service.create("changelogEntries", {
            "version": version, "tag": tag, "channel": channel,
            "status": "queued",
        })
        enqueued.append({"version": version, "tag": tag, "channel": channel, "id": eid})
    return enqueued
