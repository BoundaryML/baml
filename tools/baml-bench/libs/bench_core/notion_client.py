"""Minimal Notion REST client (ported from benchmark-builder/src/notion.rs).

Creates/updates issue pages with a title, a Status select, and a chunked
body carrying the description + evidence links.
"""

from __future__ import annotations

import os
from typing import Any, Optional

import httpx

NOTION_API = os.environ.get("NOTION_API_BASE", "https://api.notion.com/v1")
NOTION_VERSION = os.environ.get("NOTION_VERSION", "2022-06-28")
PROP_TITLE = os.environ.get("NOTION_PROP_TITLE", "Name")
PROP_STATUS = os.environ.get("NOTION_PROP_STATUS", "Status")
CHUNK = 1900


def _chunks(text: str, size: int = CHUNK) -> list[str]:
    """Split text into chunks each no larger than a size limit.

    Prefers line boundaries, but hard-splits any single line longer than the
    limit so no chunk ever exceeds ``size`` (and nothing is later truncated).

    Args:
        text: The text to split.
        size: Maximum chunk length in characters.

    Returns:
        The chunks in order; always at least one chunk (possibly empty).
    """
    out: list[str] = []
    cur = ""
    for line in text.splitlines(keepends=True):
        # Hard-split a single overlong line into size-sized pieces.
        while len(line) > size:
            if cur:
                out.append(cur)
                cur = ""
            out.append(line[:size])
            line = line[size:]
        if len(cur) + len(line) > size and cur:
            out.append(cur)
            cur = ""
        cur += line
    if cur:
        out.append(cur)
    return out or [""]


def _paragraph(text: str) -> dict[str, Any]:
    """Build a Notion paragraph block wrapping the given text.

    Args:
        text: Paragraph content; truncated to Notion's 2000-character limit.

    Returns:
        A Notion block object of type "paragraph".
    """
    return {
        "object": "block",
        "type": "paragraph",
        "paragraph": {"rich_text": [{"type": "text", "text": {"content": text[:2000]}}]},
    }


class NotionClient:
    """Minimal Notion REST client for creating and updating issue pages."""

    def __init__(self, token: str):
        """Build the auth, version, and content-type headers for Notion requests.

        Args:
            token: Notion integration token used for bearer auth.
        """
        self._headers = {
            "Authorization": f"Bearer {token}",
            "Notion-Version": NOTION_VERSION,
            "Content-Type": "application/json",
        }

    async def create_issue_page(
        self, database_id: str, title: str, status_name: str, body: str,
        evidence_links: list[str], suggestion: Optional[str] = None,
        category: Optional[str] = None,
    ) -> str:
        """Create a Notion issue page with a title, status, and chunked body.

        Assembles the body from an optional category line, the chunked
        description, an optional suggested fix, and any evidence links.

        Args:
            database_id: Id of the Notion database to add the page to.
            title: Page title; truncated to 200 characters.
            status_name: Value for the Status select property.
            body: Description text, split into paragraph blocks.
            evidence_links: Links appended as a trailing evidence paragraph.
            suggestion: Optional suggested-fix text appended as a paragraph.
            category: Optional category label prepended as a paragraph.

        Returns:
            The id of the newly created page.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        children: list[dict[str, Any]] = []
        if category:
            children.append(_paragraph(f"Category: {category}"))
        children += [_paragraph(c) for c in _chunks(body)]
        if suggestion:
            children.append(_paragraph("Suggested fix: " + suggestion))
        if evidence_links:
            children.append(_paragraph("Evidence: " + "  ".join(evidence_links)))
        payload = {
            "parent": {"database_id": database_id},
            "properties": {
                PROP_TITLE: {"title": [{"text": {"content": title[:200]}}]},
                PROP_STATUS: {"status": {"name": status_name}},
            },
            "children": children,
        }
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.post(f"{NOTION_API}/pages", json=payload, headers=self._headers)
            r.raise_for_status()
            return r.json()["id"]

    async def set_status(self, page_id: str, status_name: str) -> None:
        """Update the Status select property on an existing issue page.

        Args:
            page_id: Id of the page to update.
            status_name: New value for the Status select property.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        payload = {"properties": {PROP_STATUS: {"status": {"name": status_name}}}}
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.patch(f"{NOTION_API}/pages/{page_id}", json=payload, headers=self._headers)
            r.raise_for_status()
