"""Minimal Slack helpers (hand-rolled REST, ported from slack-bot).

post_message for threaded acks/replies and the @cursor fix ping;
verify_signature for the ingress gateway.
"""

from __future__ import annotations

import hashlib
import hmac
import os
import time
from typing import Any, Optional

import httpx

SLACK_API = os.environ.get("SLACK_API_BASE", "https://slack.com/api")


async def post_message(
    token: str,
    channel: str,
    text: str,
    *,
    thread_ts: Optional[str] = None,
    blocks: Optional[list[dict[str, Any]]] = None,
) -> Optional[str]:
    """Post a message to a Slack channel via chat.postMessage.

    Returns None without calling Slack when the token or channel is missing,
    and also when Slack reports the call was not ok.

    Args:
        token: Slack bot token used for bearer auth.
        channel: Target channel id or name.
        text: Fallback/plaintext message body.
        thread_ts: Optional parent message ts to reply in-thread.
        blocks: Optional Block Kit blocks for rich formatting.

    Returns:
        The posted message ts, or None on missing config or failure.
    """
    if not token or not channel:
        return None
    payload: dict[str, Any] = {"channel": channel, "text": text}
    if thread_ts:
        payload["thread_ts"] = thread_ts
    if blocks:
        payload["blocks"] = blocks
    async with httpx.AsyncClient(timeout=20.0) as c:
        r = await c.post(
            f"{SLACK_API}/chat.postMessage",
            json=payload,
            headers={"Authorization": f"Bearer {token}"},
        )
        data = r.json()
        if not data.get("ok"):
            return None
        return data.get("ts")


def verify_signature(
    signing_secret: str, timestamp: str, body: bytes, signature: str, *, max_skew: int = 300
) -> bool:
    """Verify a Slack request signature using the v0 HMAC-SHA256 scheme.

    Computes HMAC-SHA256 over `v0:{timestamp}:{raw_body}` and compares it to
    the provided signature; also rejects requests whose timestamp is too old.

    Args:
        signing_secret: Slack app signing secret; an empty value fails closed.
        timestamp: Request timestamp from the X-Slack-Request-Timestamp header.
        body: Raw request body bytes.
        signature: Signature from the X-Slack-Signature header.
        max_skew: Maximum allowed clock skew in seconds.

    Returns:
        True if the signature is valid and within the skew window, else False.
    """
    if not signing_secret:
        return False
    try:
        if abs(time.time() - int(timestamp)) > max_skew:
            return False
    except (ValueError, TypeError):
        return False
    base = b"v0:" + timestamp.encode() + b":" + body
    digest = hmac.new(signing_secret.encode(), base, hashlib.sha256).hexdigest()
    expected = f"v0={digest}"
    return hmac.compare_digest(expected, signature)
