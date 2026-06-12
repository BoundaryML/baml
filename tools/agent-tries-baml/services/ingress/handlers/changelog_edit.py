"""@bammy changelog-edit route: resolve a version, requeue its entry.

Ported from baml-changelog2's app/slack.py. The big architectural change:
instead of running the draft/critique loop in-process under semaphores and
per-version locks, we patch the entry row with the revise inputs and flip it
back to "queued" — the changelog_worker's claim IS the per-version lock, and
the worker posts the threaded completion reply via the slack routing stored
on the row.
"""

from __future__ import annotations

import logging
from typing import Any, Optional, Union

from bench_core import slack_client
from bench_core.service_client import ServiceClient

log = logging.getLogger("uvicorn.error")

_CHANNELS = ("nightly", "canary", "alpha", "engine")
# Statuses that mean "an edit is already in flight for this entry".
_IN_FLIGHT = ("queued", "generating")


def resolve_version(version_ref: str,
                    entries: list[dict[str, Any]]) -> Union[str, list[str], None]:
    """Map a free-text reference to a concrete stored version.

    Returns the version string on a unique match, a list of candidates when
    ambiguous, or None when nothing matches. `entries` is newest-first.

    Args:
        version_ref: The reference as the user typed it ("0.222", "latest nightly").
        entries: Existing entry rows, newest-first.

    Returns:
        A version string, a candidate list, or None.
    """
    ref = (version_ref or "").strip().lower()
    if not ref or not entries:
        return None

    # Exact match.
    for e in entries:
        if e["version"].lower() == ref:
            return e["version"]

    # "latest / most recent / newest [channel]".
    if any(w in ref for w in ("latest", "most recent", "newest", "last")):
        chan = next((c for c in _CHANNELS if c in ref), None)
        for e in entries:  # already newest-first
            if chan is None or e.get("channel") == chan:
                return e["version"]
        return None

    # Substring / prefix match (e.g. "0.222" -> the one 0.222.x).
    hits = [e["version"] for e in entries if ref in e["version"].lower()]
    if len(hits) == 1:
        return hits[0]
    if len(hits) > 1:
        return hits
    return None


async def list_entries(service: ServiceClient, limit: int = 300) -> list[dict[str, Any]]:
    """List changelog entries newest-first (all statuses).

    Args:
        service: Service client.
        limit: Maximum rows to fetch.

    Returns:
        Entry rows newest-first.
    """
    return await service.list("changelogEntries", limit=limit)


async def handle(service: ServiceClient, bot_token: str, event: dict[str, Any],
                 intent: dict[str, Any], *, allowed_users: Optional[set[str]] = None) -> None:
    """Dispatch a changelog edit: resolve the version and requeue its entry.

    Args:
        service: Service client used for reads and the requeue.
        bot_token: Slack bot token for threaded replies.
        event: The Slack event (channel, ts, thread_ts, user).
        intent: The classifier's emit_route output (version_ref, mode, guidance).
        allowed_users: Optional allowlist of Slack user ids; empty/None = open.
    """
    channel = event.get("channel")
    thread = event.get("thread_ts") or event.get("ts")
    user = event.get("user")

    async def reply(text: str) -> None:
        await slack_client.post_message(bot_token, channel, text, thread_ts=thread)

    # Authorization: these edits hit the live site. Empty allowlist = open.
    if allowed_users and user not in allowed_users:
        log.info("bammy: refusing changelog edit from user=%s (not allowlisted)", user)
        await reply("Sorry, you're not on the changelog edit allowlist.")
        return

    entries = await list_entries(service)
    resolved = resolve_version(intent.get("version_ref", ""), entries)
    if resolved is None:
        await reply(
            f"I couldn't find an entry matching \"{intent.get('version_ref')}\". "
            "Which version? (e.g. `0.222.0`)"
        )
        return
    if isinstance(resolved, list):
        opts = ", ".join(f"`{v}`" for v in resolved[:8])
        await reply(f"Did you mean one of: {opts}? Reply with the exact version.")
        return

    entry = next((e for e in entries if e["version"] == resolved), None)
    if entry is None:  # cannot happen after resolve, but stay safe
        await reply(f"Lost track of `{resolved}`, try again.")
        return
    if entry.get("status") in _IN_FLIGHT:
        await reply(f"Already working on `{resolved}`, hang on.")
        return

    mode = intent.get("mode", "revise")
    guidance = (intent.get("guidance") or "").strip()
    patch: dict[str, Any] = {
        "reviseMode": mode,
        "reviseGuidance": guidance if mode == "revise" else None,
        "slackChannel": channel,
        "slackThreadTs": thread,
        "slackUser": user,
    }
    await service.update("changelogEntries", entry["_id"], patch)
    await service.transition("changelogEntries", entry["_id"], "queued")
    verb = "Regenerating" if mode == "regenerate" else "Revising"
    await reply(f"{verb} `{resolved}`... this takes a couple minutes.")
    log.info("bammy: requeued changelog entry %s mode=%s", resolved, mode)
