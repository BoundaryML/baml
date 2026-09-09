#!/usr/bin/env -S uv run --script
# Run from the repository root. Use `upload FILE` to create a campaign, or `download` to fetch every Sheep Council campaign.
# Add --campaign-id ID and optionally --email-message-id ID to update an existing campaign.
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any

LOOPS_API_BASE_URL = "https://app.loops.so/api"
LOOPS_CAMPAIGN_COMPOSE_QUERY = "stepName=Compose&page=0&pageSize=20&sortMetricsBy=emailCreatedAt&sortMetricsOrder=desc&columnPinning=email"
SHEEP_COUNCIL_CAMPAIGN_GROUP_ID = "cmtc7a2fx078l0j4jwqydy4m4"
DEFAULT_DOWNLOAD_DIR = Path(__file__).with_name("email-data") / "downloads"
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


def list_sheep_council_campaigns(api_key: str) -> list[dict[str, Any]]:
    campaigns: list[dict[str, Any]] = []
    cursor: str | None = None
    while True:
        query = {"perPage": "50"}
        if cursor:
            query["cursor"] = cursor
        page = request(api_key, f"/v1/campaigns?{urllib.parse.urlencode(query)}")
        campaigns.extend(
            campaign
            for campaign in page["data"]
            if campaign["campaignGroupId"] == SHEEP_COUNCIL_CAMPAIGN_GROUP_ID
        )
        cursor = page["pagination"]["nextCursor"]
        if not cursor:
            return campaigns


def lmx_source(campaign: dict[str, Any], message: dict[str, Any]) -> str:
    values = {
        **{field: campaign[field] for field in CAMPAIGN_FIELDS},
        **{field: message[field] for field in MESSAGE_FIELDS},
    }
    frontmatter = "\n".join(
        f"{field}: {json.dumps(value, ensure_ascii=False, separators=(',', ':'))}"
        for field, value in values.items()
    )
    return f"---\n{frontmatter}\n---\n{message['lmx'].strip()}\n"


def markdown_node(node: ET.Element) -> str:
    content = node.text or ""
    for child in node:
        content += markdown_node(child)
        content += child.tail or ""
    if node.tag == "Style":
        return ""
    if node.tag == "Paragraph":
        return f"{content.strip()}\n\n" if content.strip() else "\n"
    if node.tag == "UnorderedList":
        return "".join(f"- {markdown_node(child).strip()}\n" for child in node) + "\n"
    if node.tag == "ListItem":
        return content
    if node.tag == "Strong":
        return f"**{content}**"
    if node.tag == "Code":
        return f"`{content}`"
    if node.tag == "Link":
        return f"[{content}]({node.attrib['href']})"
    if node.tag == "Text" and node.attrib.get("textColor"):
        return f'<span style="color: {node.attrib["textColor"]}">{content}</span>'
    if node.tag == "Br":
        return "\n"
    return content


def markdown_preview(campaign: dict[str, Any], message: dict[str, Any]) -> str:
    root = ET.fromstring(f"<Root>{message['lmx']}</Root>")
    body = re.sub(
        r"\n{3,}", "\n\n", "".join(markdown_node(child) for child in root)
    ).strip()
    metadata = {
        "campaignId": campaign["id"],
        "campaignUrl": campaign["url"],
        "subject": message["subject"],
        "previewText": message["previewText"],
        "fromName": message["fromName"],
        "fromEmail": message["fromEmail"],
        "replyToEmail": message["replyToEmail"],
    }
    frontmatter = "\n".join(
        f"{field}: {json.dumps(value, ensure_ascii=False)}"
        for field, value in metadata.items()
    )
    return f"---\n{frontmatter}\n---\n{body}\n"


def campaign_file_stem(campaign: dict[str, Any]) -> str:
    created_date = campaign["createdAt"][:10]
    slug = re.sub(r"[^a-z0-9]+", "-", campaign["name"].lower()).strip("-")
    return f"{created_date}-{slug}-{campaign['id']}"


def campaign_compose_url(campaign_id: str) -> str:
    return f"https://app.loops.so/campaigns/{campaign_id}/compose?{LOOPS_CAMPAIGN_COMPOSE_QUERY}"


def download(api_key: str, output_dir: Path) -> list[dict[str, str]]:
    output_dir.mkdir(parents=True, exist_ok=True)
    downloaded = []
    for campaign in list_sheep_council_campaigns(api_key):
        message = request(api_key, f"/v1/email-messages/{campaign['emailMessageId']}")
        stem = campaign_file_stem(campaign)
        lmx_path = output_dir / f"{stem}.lmx"
        markdown_path = output_dir / f"{stem}.md"
        lmx_path.write_text(lmx_source(campaign, message))
        markdown_path.write_text(markdown_preview(campaign, message))
        downloaded.append(
            {
                "campaignId": campaign["id"],
                "campaignUrl": campaign["url"],
                "lmx": str(lmx_path),
                "markdown": str(markdown_path),
            }
        )
    return downloaded


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
        "composeUrl": campaign_compose_url(campaign_id),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Manage Sheep Council emails in Loops."
    )
    commands = parser.add_subparsers(dest="command", required=True)
    upload_parser = commands.add_parser("upload", help="Upload an LMX email.")
    upload_parser.add_argument("file", type=Path)
    upload_parser.add_argument("--campaign-id")
    upload_parser.add_argument("--email-message-id")
    download_parser = commands.add_parser(
        "download", help="Download all Sheep Council campaign emails."
    )
    download_parser.add_argument(
        "--output-dir", type=Path, default=DEFAULT_DOWNLOAD_DIR
    )
    args = parser.parse_args()
    api_key = os.environ["LOOPS_EMAIL_CAMPAIGNS_API_KEY"]
    if args.command == "download":
        result = download(api_key, args.output_dir)
        print(json.dumps(result, indent=2))
        return
    if args.email_message_id and not args.campaign_id:
        parser.error("--email-message-id requires --campaign-id")
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
