"""Thin wrapper around slack_sdk for oncall posting."""

from __future__ import annotations

import datetime
import os
from typing import Any

from slack_sdk import WebClient


def client() -> WebClient:
    token = os.environ.get("SLACK_BOUNDARY_BOT_TOKEN")
    if not token:
        raise RuntimeError("SLACK_BOUNDARY_BOT_TOKEN not set")
    return WebClient(token=token)


def lookup_user_id(wc: WebClient, email: str) -> str:
    resp = wc.users_lookupByEmail(email=email)
    return resp["user"]["id"]


def post(
    wc: WebClient,
    channel: str,
    text: str,
    *,
    blocks: list[dict[str, Any]] | None = None,
) -> None:
    wc.chat_postMessage(
        channel=channel,
        text=text,
        blocks=blocks,
        unfurl_links=False,
        unfurl_media=False,
    )


def schedule(
    wc: WebClient,
    channel: str,
    text: str,
    post_at: datetime.datetime,
    *,
    blocks: list[dict[str, Any]] | None = None,
) -> None:
    """Schedule a message for later delivery via chat.scheduleMessage.

    Slack requires `post_at` to be in the future and within 120 days.
    """
    if post_at.tzinfo is None:
        raise ValueError("post_at must be timezone-aware")
    wc.chat_scheduleMessage(
        channel=channel,
        post_at=int(post_at.timestamp()),
        text=text,
        blocks=blocks,
        unfurl_links=False,
        unfurl_media=False,
    )


def email_for(name: str) -> str:
    return f"{name}@boundaryml.com"
