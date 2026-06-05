"""Minimal Notion REST client (ported from benchmark-builder/src/notion.rs).

Creates/updates issue pages with a title, a Status select, and a structured
body (Markdown description -> Notion blocks, a verified-repro code block,
suggested fix, and evidence bullets), and reads page status + comments back.
"""

from __future__ import annotations

import os
import re
from typing import Any, Optional

import httpx

from bench_core.md_to_notion import code_block, markdown_to_blocks

NOTION_API = os.environ.get("NOTION_API_BASE", "https://api.notion.com/v1")
NOTION_VERSION = os.environ.get("NOTION_VERSION", "2022-06-28")
PROP_TITLE = os.environ.get("NOTION_PROP_TITLE", "Name")
PROP_STATUS = os.environ.get("NOTION_PROP_STATUS", "Status")


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

    @staticmethod
    def _issue_children(
        body: str, evidence_links: list[str], suggestion: Optional[str],
        category: Optional[str], repro: Optional[str],
    ) -> list[dict[str, Any]]:
        """Build the Notion blocks for an issue page.

        Renders the description through the Markdown converter and adds a
        Reproduction code block, a Suggested fix section, and an Evidence bullet
        list when those are present.

        Args:
            body: Markdown description; converted to structured blocks.
            evidence_links: Links rendered as an Evidence bullet list.
            suggestion: Optional suggested-fix Markdown.
            category: Optional category label rendered as a bold line.
            repro: Optional verified repro, rendered verbatim in a code block.

        Returns:
            The ordered list of Notion block objects.
        """
        children: list[dict[str, Any]] = []
        if category:
            children += markdown_to_blocks(f"**Category:** {category}")
        children += markdown_to_blocks(body or "")
        if repro:
            children += markdown_to_blocks("## Reproduction")
            # BAML reads close to TypeScript, so highlight the repro as such.
            children.append(code_block(repro, "typescript"))
        if suggestion:
            children += markdown_to_blocks("## Suggested fix")
            children += markdown_to_blocks(suggestion)
        if evidence_links:
            items = []
            for i, link in enumerate(evidence_links, 1):
                call = re.search(r"call=(\d+)", link)
                label = f"run {i}" + (f" · call {call.group(1)}" if call else "")
                items.append(f"- [{label}]({link})")
            children += markdown_to_blocks("## Evidence\n" + "\n".join(items))
        return children

    async def create_issue_page(
        self, database_id: str, title: str, status_name: str, body: str,
        evidence_links: list[str], suggestion: Optional[str] = None,
        category: Optional[str] = None, repro: Optional[str] = None,
    ) -> str:
        """Create a structured Notion issue page from Markdown.

        Assembles the page from an optional category line, the Markdown
        description (converted to headings/lists/code), an optional verified
        repro code block, an optional suggested fix, and an evidence bullet list.
        Children past Notion's 100-per-request limit are appended in batches.

        Args:
            database_id: Id of the Notion database to add the page to.
            title: Page title; truncated to 200 characters.
            status_name: Value for the Status select property.
            body: Markdown description, converted to structured blocks.
            evidence_links: Links rendered as an Evidence bullet list.
            suggestion: Optional suggested-fix Markdown.
            category: Optional category label.
            repro: Optional verified repro rendered as a code block.

        Returns:
            The id of the newly created page.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        children = self._issue_children(body, evidence_links, suggestion, category, repro)
        payload = {
            "parent": {"database_id": database_id},
            "properties": {
                PROP_TITLE: {"title": [{"text": {"content": title[:200]}}]},
                PROP_STATUS: {"status": {"name": status_name}},
            },
            "children": children[:100],
        }
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.post(f"{NOTION_API}/pages", json=payload, headers=self._headers)
            r.raise_for_status()
            page_id = r.json()["id"]
        if len(children) > 100:
            await self._append_children(page_id, children[100:])
        return page_id

    async def _append_children(self, page_id: str, blocks: list[dict[str, Any]]) -> None:
        """Append child blocks to a page in <=100-block batches.

        Args:
            page_id: Id of the page (block) to append children to.
            blocks: The block objects to append, in order.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        async with httpx.AsyncClient(timeout=30.0) as c:
            for start in range(0, len(blocks), 100):
                r = await c.patch(
                    f"{NOTION_API}/blocks/{page_id}/children",
                    json={"children": blocks[start : start + 100]},
                    headers=self._headers,
                )
                r.raise_for_status()

    async def _replace_children(self, page_id: str, blocks: list[dict[str, Any]]) -> None:
        """Replace a page's body: archive all existing child blocks, append new.

        Args:
            page_id: Id of the page whose body to replace.
            blocks: The new block objects, in order.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        async with httpx.AsyncClient(timeout=30.0) as c:
            existing: list[str] = []
            params: dict[str, Any] = {"page_size": 100}
            while True:
                r = await c.get(
                    f"{NOTION_API}/blocks/{page_id}/children", params=params, headers=self._headers
                )
                r.raise_for_status()
                data = r.json()
                existing += [b["id"] for b in data.get("results", [])]
                if not data.get("has_more"):
                    break
                params["start_cursor"] = data.get("next_cursor")
            for bid in existing:
                dr = await c.delete(f"{NOTION_API}/blocks/{bid}", headers=self._headers)
                dr.raise_for_status()
        await self._append_children(page_id, blocks)

    async def update_issue_page(
        self, page_id: str, title: str, status_name: str, body: str,
        evidence_links: list[str], suggestion: Optional[str] = None,
        category: Optional[str] = None, repro: Optional[str] = None,
    ) -> None:
        """Re-render an existing issue page in place: title, status, and body.

        Used so a redrafted issue (or one whose description/repro/evidence
        changed) actually updates its Notion card, not just its status. Updates
        the title + Status properties, then replaces the page body.

        Args:
            page_id: Id of the page to update.
            title: New page title; truncated to 200 characters.
            status_name: New value for the Status select property.
            body: Markdown description, converted to structured blocks.
            evidence_links: Links rendered as an Evidence bullet list.
            suggestion: Optional suggested-fix Markdown.
            category: Optional category label.
            repro: Optional verified repro rendered as a code block.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        payload = {"properties": {
            PROP_TITLE: {"title": [{"text": {"content": title[:200]}}]},
            PROP_STATUS: {"status": {"name": status_name}},
        }}
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.patch(f"{NOTION_API}/pages/{page_id}", json=payload, headers=self._headers)
            r.raise_for_status()
        await self._replace_children(
            page_id, self._issue_children(body, evidence_links, suggestion, category, repro)
        )

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

    async def get_page(self, page_id: str) -> dict[str, Any]:
        """Fetch a Notion page object.

        Args:
            page_id: Id of the page to retrieve.

        Returns:
            The page object as returned by Notion (includes ``properties``).

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.get(f"{NOTION_API}/pages/{page_id}", headers=self._headers)
            r.raise_for_status()
            return r.json()

    async def get_status(self, page_id: str) -> Optional[str]:
        """Read the current Status select value of a page.

        Args:
            page_id: Id of the page whose status to read.

        Returns:
            The Status select name, or None when the property is unset/missing.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        page = await self.get_page(page_id)
        prop = (page.get("properties") or {}).get(PROP_STATUS) or {}
        status = prop.get("status") or {}
        return status.get("name")

    async def get_comments(self, page_id: str) -> list[dict[str, Any]]:
        """Fetch all comments on a page, following pagination.

        Args:
            page_id: Id of the page (block) whose comments to fetch.

        Returns:
            A list of ``{"text", "created_time", "author"}`` dicts in Notion
            order. Comment text is the concatenation of its rich_text plain_text.

        Raises:
            httpx.HTTPStatusError: If Notion returns a non-2xx response.
        """
        out: list[dict[str, Any]] = []
        params: dict[str, Any] = {"block_id": page_id}
        async with httpx.AsyncClient(timeout=30.0) as c:
            while True:
                r = await c.get(f"{NOTION_API}/comments", params=params, headers=self._headers)
                r.raise_for_status()
                data = r.json()
                for cm in data.get("results", []):
                    text = "".join(
                        rt.get("plain_text", "") for rt in (cm.get("rich_text") or [])
                    )
                    out.append({
                        "text": text,
                        "created_time": cm.get("created_time"),
                        "author": (cm.get("created_by") or {}).get("id"),
                    })
                if not data.get("has_more"):
                    break
                params["start_cursor"] = data.get("next_cursor")
        return out
