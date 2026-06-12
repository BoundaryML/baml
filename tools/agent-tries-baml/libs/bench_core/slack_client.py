"""Minimal Slack helpers (hand-rolled REST, ported from slack-bot).

post_message for threaded acks/replies and the @cursor fix ping;
verify_signature for the ingress gateway; fetch_thread/users_info for
the @bammy router's thread-context reading and promo audit trail.
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
    try:
        async with httpx.AsyncClient(timeout=20.0) as c:
            r = await c.post(
                f"{SLACK_API}/chat.postMessage",
                json=payload,
                headers={"Authorization": f"Bearer {token}"},
            )
            data = r.json()
    except (httpx.HTTPError, ValueError):  # network failure or non-JSON outage page
        return None
    if not data.get("ok"):
        return None
    return data.get("ts")


async def fetch_thread(
    token: str, channel: str, thread_ts: str, *, limit: int = 30
) -> list[dict[str, Any]]:
    """Fetch the messages of a Slack thread via conversations.replies.

    Used by the @bammy router so a mid-thread mention can read what was said
    before the mention. Requires the *:history scope matching the channel type.

    Args:
        token: Slack bot token used for bearer auth.
        channel: Channel id the thread lives in.
        thread_ts: ts of the thread's parent message.
        limit: Maximum number of messages to fetch (oldest-first).

    Returns:
        The thread's message objects oldest-first (parent included), or an
        empty list on missing config or failure.
    """
    if not token or not channel or not thread_ts:
        return []
    try:
        async with httpx.AsyncClient(timeout=20.0) as c:
            r = await c.get(
                f"{SLACK_API}/conversations.replies",
                params={"channel": channel, "ts": thread_ts, "limit": limit},
                headers={"Authorization": f"Bearer {token}"},
            )
            data = r.json()
    except (httpx.HTTPError, ValueError):
        return []
    if not data.get("ok"):
        return []
    return data.get("messages") or []


async def users_info(token: str, user_id: str) -> Optional[dict[str, Any]]:
    """Fetch a Slack user's profile via users.info.

    Args:
        token: Slack bot token used for bearer auth.
        user_id: Slack user id (U...).

    Returns:
        The user object, or None on missing config or failure.
    """
    if not token or not user_id:
        return None
    try:
        async with httpx.AsyncClient(timeout=20.0) as c:
            r = await c.get(
                f"{SLACK_API}/users.info",
                params={"user": user_id},
                headers={"Authorization": f"Bearer {token}"},
            )
            data = r.json()
    except (httpx.HTTPError, ValueError):
        return None
    if not data.get("ok"):
        return None
    return data.get("user")


def display_name(user: Optional[dict[str, Any]]) -> str:
    """Best display name for a Slack user object (profile display name,
    then real name, then the raw user id), mirroring the t-shirts bot.

    Args:
        user: A users.info user object, or None.

    Returns:
        A non-empty human-readable name, or "" when user is None.
    """
    if not user:
        return ""
    profile = user.get("profile") or {}
    return (
        profile.get("display_name")
        or profile.get("real_name")
        or user.get("real_name")
        or user.get("name")
        or user.get("id")
        or ""
    )


def render_thread(messages: list[dict[str, Any]], *, names: Optional[dict[str, str]] = None) -> str:
    """Render thread messages as "name: text" lines for prompt context.

    Args:
        messages: Message objects from fetch_thread (oldest-first).
        names: Optional user-id -> display-name map; unmapped ids render raw.

    Returns:
        One line per non-empty message; bot messages are tagged "[bot]".
    """
    names = names or {}
    lines: list[str] = []
    for m in messages:
        text = (m.get("text") or "").strip()
        if not text:
            continue
        if m.get("bot_id") and not m.get("user"):
            who = "[bot]"
        else:
            uid = m.get("user") or "unknown"
            who = names.get(uid, uid)
        lines.append(f"{who}: {text}")
    return "\n".join(lines)


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
