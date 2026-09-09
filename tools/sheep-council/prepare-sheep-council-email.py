#!/usr/bin/env -S uv run --script
# Run from the repository root to create a campaign:
#   infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=dev-humans -- uv run tools/sheep-council/prepare-sheep-council-email.py tools/sheep-council/email-data/my-email.lmx
# To update an existing campaign, add --campaign-id ID and optionally --email-message-id ID.
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import os
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

LOOPS_API_BASE_URL = "https://app.loops.so/api"
CAMPAIGN_FIELDS = ("name", "campaignGroupId", "mailingListId")
MESSAGE_FIELDS = (
    "subject",
    "previewText",
    "fromName",
    "fromEmail",
    "replyToEmail",
    "emailFormat",
    "contactPropertiesFallbacks",
)


def parse_source(source: str) -> tuple[dict[str, Any], dict[str, Any]]:
    lines = source.replace("\r\n", "\n").split("\n")
    end = lines.index("---", 1)
    frontmatter: dict[str, Any] = {}
    for line in lines[1:end]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, raw_value = line.split(":", 1)
        try:
            frontmatter[key] = json.loads(raw_value.strip())
        except json.JSONDecodeError:
            frontmatter[key] = raw_value.strip()
    campaign = {field: frontmatter[field] for field in CAMPAIGN_FIELDS}
    message = {field: frontmatter[field] for field in MESSAGE_FIELDS}
    message["lmx"] = "\n".join(lines[end + 1 :]).strip()
    return campaign, message


def request(
    api_key: str,
    path: str,
    method: str = "GET",
    body: dict[str, Any] | None = None,
) -> dict[str, Any]:
    api_request = urllib.request.Request(
        f"{LOOPS_API_BASE_URL}{path}",
        data=None if body is None else json.dumps(body).encode(),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json; charset=utf-8",
            "User-Agent": "baml-sheep-council-tool/1.0",
        },
        method=method,
    )
    try:
        with urllib.request.urlopen(api_request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        detail = error.read().decode(errors="replace")
        raise RuntimeError(
            f"Loops request {method} {path} failed ({error.code}): {detail}"
        ) from error


def upload(
    api_key: str,
    campaign_input: dict[str, Any],
    message_input: dict[str, Any],
    campaign_id: str | None,
    email_message_id: str | None,
) -> dict[str, Any]:
    if campaign_id:
        campaign = request(
            api_key, f"/v1/campaigns/{campaign_id}", "POST", campaign_input
        )
        email_message_id = email_message_id or campaign["emailMessageId"]
        revision_id = request(api_key, f"/v1/email-messages/{email_message_id}")[
            "contentRevisionId"
        ]
        action = "update"
    else:
        campaign = request(api_key, "/v1/campaigns", "POST", campaign_input)
        campaign_id = campaign["id"]
        email_message_id = campaign["emailMessageId"]
        revision_id = campaign["emailMessageContentRevisionId"]
        action = "create"
    request(
        api_key,
        f"/v1/email-messages/{email_message_id}",
        "POST",
        {"expectedRevisionId": revision_id, **message_input},
    )
    return {
        "action": action,
        "campaignId": campaign_id,
        "campaignUrl": campaign["url"],
        "emailMessageId": email_message_id,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Upload an LMX email to Loops.")
    parser.add_argument("file", type=Path)
    parser.add_argument("--campaign-id")
    parser.add_argument("--email-message-id")
    args = parser.parse_args()
    if args.email_message_id and not args.campaign_id:
        parser.error("--email-message-id requires --campaign-id")
    api_key = os.environ["LOOPS_EMAIL_CAMPAIGNS_API_KEY"]
    campaign, message = parse_source(args.file.read_text())
    result = upload(
        api_key,
        campaign,
        message,
        args.campaign_id,
        args.email_message_id,
    )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
