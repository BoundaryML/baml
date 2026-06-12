"""@bammy promo-code route (absorbs the growth/t-shirts bot).

Claims the next unused code via one OCC mutation and replies in-thread with
the code plus an audit line, matching the old bot's UX.
"""

from __future__ import annotations

import logging
from typing import Any

from bench_core import slack_client
from bench_core.service_client import ServiceClient

log = logging.getLogger("uvicorn.error")


async def handle(service: ServiceClient, bot_token: str, event: dict[str, Any],
                 intent: dict[str, Any]) -> None:
    """Claim a promo code for the mentioning user and reply in-thread.

    Args:
        service: Service client used for the claim.
        bot_token: Slack bot token for the reply and the users.info lookup.
        event: The Slack event (channel, ts, thread_ts, user).
        intent: The classifier's emit_route output (promo_notes).
    """
    channel = event.get("channel")
    thread = event.get("thread_ts") or event.get("ts")
    user_id = event.get("user") or ""

    async def reply(text: str) -> None:
        await slack_client.post_message(bot_token, channel, text, thread_ts=thread)

    user = await slack_client.users_info(bot_token, user_id)
    name = slack_client.display_name(user) or user_id
    notes = (intent.get("promo_notes") or "").strip() or (event.get("text") or "")[:300]

    code = await service.promo_claim(name, user_id, notes)
    if code is None:
        await reply("We're out of codes! Ping the growth team to load more.")
        log.warning("bammy: promo inventory exhausted (request by %s)", name)
        return
    await reply(f"`{code}` — logged for *{name}*. One per person, enjoy the shirt!")
    log.info("bammy: issued promo code to %s (%s)", name, user_id)
