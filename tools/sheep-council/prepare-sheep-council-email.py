#!/usr/bin/env -S uv run --script
# Run from the repository root:
#   infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=dev-humans -- uv run tools/sheep-council/prepare-sheep-council-email.py
# Add --file PATH to select an LMX source and --apply to create or update the draft. The command never schedules or sends it.
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

LOOPS_API_BASE_URL = "https://app.loops.so/api"
DEFAULT_SOURCE = Path(__file__).with_name("email-data") / "template.lmx"
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


def parse_frontmatter(source: str) -> tuple[dict[str, Any], str]:
    lines = source.replace("\r\n", "\n").split("\n")
    if not lines or lines[0] != "---":
        raise ValueError("Email draft must begin with frontmatter")
    try:
        end = lines.index("---", 1)
    except ValueError as error:
        raise ValueError("Email draft frontmatter is not closed") from error
    frontmatter: dict[str, Any] = {}
    for line in lines[1:end]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        match = re.fullmatch(r"([A-Za-z][A-Za-z0-9]*):\s*(.+)", line)
        if not match:
            raise ValueError(f"Unsupported frontmatter line: {line}")
        key, raw_value = match.groups()
        if key in frontmatter:
            raise ValueError(f"Duplicate frontmatter key: {key}")
        try:
            frontmatter[key] = json.loads(raw_value)
        except json.JSONDecodeError:
            frontmatter[key] = raw_value.strip()
    lmx = "\n".join(lines[end + 1 :]).strip()
    if not lmx:
        raise ValueError("Email LMX cannot be empty")
    return frontmatter, lmx


def required_string(values: dict[str, Any], key: str) -> str:
    value = values.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"frontmatter.{key} must be a non-empty string")
    return value.strip()


def parse_source(source: str) -> dict[str, Any]:
    frontmatter, lmx = parse_frontmatter(source)
    allowed_fields = set(CAMPAIGN_FIELDS + MESSAGE_FIELDS)
    unknown_fields = set(frontmatter) - allowed_fields
    if unknown_fields:
        raise ValueError(
            f"Unsupported frontmatter fields: {', '.join(sorted(unknown_fields))}"
        )
    campaign = {field: required_string(frontmatter, field) for field in CAMPAIGN_FIELDS}
    message = {
        field: required_string(frontmatter, field)
        for field in MESSAGE_FIELDS
        if field not in ("contactPropertiesFallbacks", "previewText")
    }
    preview_text = frontmatter.get("previewText")
    if not isinstance(preview_text, str):
        raise TypeError("frontmatter.previewText must be a string")
    message["previewText"] = preview_text
    fallbacks = frontmatter.get("contactPropertiesFallbacks")
    if not isinstance(fallbacks, dict) or not all(
        isinstance(key, str) and isinstance(value, str)
        for key, value in fallbacks.items()
    ):
        raise ValueError(
            "frontmatter.contactPropertiesFallbacks must be an object of strings"
        )
    message["contactPropertiesFallbacks"] = fallbacks
    message["lmx"] = lmx
    if message["emailFormat"] not in ("styled", "plain"):
        raise ValueError("frontmatter.emailFormat must be styled or plain")
    if "@" in message["fromEmail"]:
        raise ValueError(
            "frontmatter.fromEmail must be the local part configured in Loops"
        )
    if message["emailFormat"] == "styled" and not lmx.startswith("<Style "):
        raise ValueError("Styled email LMX must begin with a <Style /> element")
    return {"campaign": campaign, "message": message}


class LoopsClient:
    def __init__(self, api_key: str) -> None:
        self.api_key = api_key

    def request(
        self, path: str, method: str = "GET", body: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        request = urllib.request.Request(
            f"{LOOPS_API_BASE_URL}{path}",
            data=None if body is None else json.dumps(body).encode(),
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json; charset=utf-8",
                "User-Agent": "baml-sheep-council-tool/1.0",
            },
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.load(response)
        except urllib.error.HTTPError as error:
            response_body = error.read().decode(errors="replace")
            try:
                detail = json.loads(response_body)
                detail = detail.get("message") or detail.get("error") or detail
            except json.JSONDecodeError:
                detail = response_body
            raise RuntimeError(
                f"Loops request {method} {path} failed ({error.code}): {detail}"
            ) from error
        if not isinstance(result, dict):
            raise TypeError(f"Loops returned an invalid response for {path}")
        return result

    def list_campaigns(self) -> list[dict[str, Any]]:
        campaigns: list[dict[str, Any]] = []
        cursor: str | None = None
        seen_cursors: set[str] = set()
        while True:
            query = {"perPage": "50"}
            if cursor:
                query["cursor"] = cursor
            result = self.request(f"/v1/campaigns?{urllib.parse.urlencode(query)}")
            page = result.get("data")
            pagination = result.get("pagination")
            if not isinstance(page, list) or not isinstance(pagination, dict):
                raise TypeError("Loops returned invalid campaign pagination")
            campaigns.extend(page)
            cursor = pagination.get("nextCursor")
            if cursor is None:
                return campaigns
            if not isinstance(cursor, str) or cursor in seen_cursors:
                raise RuntimeError("Loops returned an invalid campaign cursor")
            seen_cursors.add(cursor)


def choose_campaign(
    campaigns: list[dict[str, Any]], campaign: dict[str, str]
) -> dict[str, Any] | None:
    matches = [
        item
        for item in campaigns
        if item.get("campaignGroupId") == campaign["campaignGroupId"]
        and item.get("name") == campaign["name"]
    ]
    drafts = [item for item in matches if item.get("status") == "Draft"]
    if len(drafts) > 1:
        raise RuntimeError(
            f"Found {len(drafts)} matching Loops drafts; refusing to choose one"
        )
    if drafts:
        return drafts[0]
    if matches:
        statuses = ", ".join(str(item.get("status")) for item in matches)
        raise RuntimeError(
            f"Campaign {campaign['name']!r} already exists with status {statuses}"
        )
    return None


def prepare(client: LoopsClient, spec: dict[str, Any], apply: bool) -> dict[str, Any]:
    campaign_input = spec["campaign"]
    existing = choose_campaign(client.list_campaigns(), campaign_input)
    action = "update" if existing else "create"
    if not apply:
        return {
            "action": action,
            "applied": False,
            "campaignId": existing.get("id") if existing else None,
            "campaignUrl": existing.get("url") if existing else None,
        }
    if existing:
        campaign_id = existing.get("id")
        email_message_id = existing.get("emailMessageId")
        if not isinstance(campaign_id, str) or not isinstance(email_message_id, str):
            raise RuntimeError("Matching draft has no editable email message")
        message = client.request(f"/v1/email-messages/{email_message_id}")
        revision_id = message.get("contentRevisionId")
        campaign = client.request(
            f"/v1/campaigns/{campaign_id}", "POST", campaign_input
        )
    else:
        campaign = client.request("/v1/campaigns", "POST", campaign_input)
        email_message_id = campaign.get("emailMessageId")
        revision_id = campaign.get("emailMessageContentRevisionId")
    if not isinstance(email_message_id, str) or not isinstance(revision_id, str):
        raise TypeError("Loops draft has no editable email revision")
    client.request(
        f"/v1/email-messages/{email_message_id}",
        "POST",
        {"expectedRevisionId": revision_id, **spec["message"]},
    )
    return {
        "action": action,
        "applied": True,
        "campaignId": campaign.get("id"),
        "campaignUrl": campaign.get("url"),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Create or update the next Sheep Council Loops draft."
    )
    parser.add_argument("--file", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    source_path = args.file.resolve()
    source = source_path.read_text()
    if args.apply and re.search(r"TODO", source, re.IGNORECASE):
        raise ValueError(
            "The LMX email draft still contains TODO; finish editing it before --apply"
        )
    api_key = os.environ.get("LOOPS_EMAIL_CAMPAIGNS_API_KEY")
    if not api_key:
        raise RuntimeError(
            "LOOPS_EMAIL_CAMPAIGNS_API_KEY is required; use the dev-humans Infisical environment"
        )
    spec = parse_source(source)
    result = prepare(LoopsClient(api_key), spec, args.apply)
    print(json.dumps({"source": str(source_path), **result, "spec": spec}, indent=2))
    if not args.apply:
        print(
            "Dry-run only; pass --apply to write this draft to Loops.", file=sys.stderr
        )


if __name__ == "__main__":
    main()
