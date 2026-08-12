"""Thin wrapper around slack_sdk for oncall posting."""

from __future__ import annotations

import os

from slack_sdk import WebClient


def client() -> WebClient:
    token = os.environ.get("SLACK_BOUNDARY_BOT_TOKEN")
    if not token:
        raise RuntimeError("SLACK_BOUNDARY_BOT_TOKEN not set")
    return WebClient(token=token)


def lookup_user_id(wc: WebClient, email: str) -> str:
    resp = wc.users_lookupByEmail(email=email)
    return resp["user"]["id"]


def post(wc: WebClient, channel: str, text: str) -> None:
    wc.chat_postMessage(
        channel=channel,
        text=text,
        unfurl_links=False,
        unfurl_media=False,
    )


def email_for(name: str) -> str:
    return f"{name}@boundaryml.com"
