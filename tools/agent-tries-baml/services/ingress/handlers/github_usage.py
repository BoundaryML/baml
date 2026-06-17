"""@bammy "how is BAML used on GitHub" -> trigger an on-demand usage scan.

The GitHub-usage tracker is a separate app (`baml-usage`) with its own Convex
database. This handler posts an ack in-thread and asks that service — reached
over the Fly private network — to run a scan; the tracker posts the full report
back into the same thread when it finishes (a few minutes later).
"""

from __future__ import annotations

import logging
import os
from typing import Any

import httpx

from bench_core import slack_client
from bench_core.service_client import ServiceClient

log = logging.getLogger("uvicorn.error")

# The baml-usage trigger server, reached over the Fly 6PN private network.
USAGE_URL = os.environ.get("BAML_USAGE_URL", "http://baml-usage.internal:8080")
# Shared secret with the baml-usage app (optional; empty = no auth).
TRIGGER_TOKEN = os.environ.get("BAML_USAGE_TRIGGER_TOKEN", "")


async def handle(
    service: ServiceClient, bot_token: str, event: dict[str, Any], intent: dict[str, Any]
) -> None:
    """Kick off a GitHub-usage scan and ack in-thread.

    Args:
        service: The ingress ServiceClient (unused; kept for handler symmetry).
        bot_token: Slack bot token to reply with.
        event: The Slack app_mention event.
        intent: The classified route object.
    """
    channel = event.get("channel")
    thread = event.get("thread_ts") or event.get("ts")
    headers = {"Authorization": f"Bearer {TRIGGER_TOKEN}"} if TRIGGER_TOKEN else {}
    try:
        async with httpx.AsyncClient(timeout=20.0) as c:
            r = await c.post(
                f"{USAGE_URL}/scan",
                json={"channel": channel, "thread_ts": thread},
                headers=headers,
            )
            r.raise_for_status()
    except Exception:  # noqa: BLE001 — surface the failure rather than going silent
        log.exception("bammy: github usage trigger failed")
        await slack_client.post_message(
            bot_token, channel,
            "Sorry — I couldn't kick off the GitHub usage scan just now.",
            thread_ts=thread,
        )
        return
    await slack_client.post_message(
        bot_token, channel,
        "On it — scanning public GitHub for how BAML is being used. "
        "I'll post the report here when it's ready (usually a few minutes).",
        thread_ts=thread,
    )
