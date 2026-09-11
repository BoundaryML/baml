"""Thin wrapper around slack_sdk for oncall posting."""

from __future__ import annotations

import os
from typing import Any

from slack_sdk import WebClient


def client(*, retry_handlers=None) -> WebClient:
    token = os.environ.get("SLACK_BOUNDARY_BOT_TOKEN")
    if not token:
        raise RuntimeError("SLACK_BOUNDARY_BOT_TOKEN not set")
    return WebClient(token=token, retry_handlers=retry_handlers)


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


def email_for(name: str) -> str:
    return f"{name}@boundaryml.com"
