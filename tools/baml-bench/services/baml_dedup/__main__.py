"""BAML dedup processor: batch trophies, run the classify/dedup agent, upsert the
issues DB (the authoritative skill-vs-language classifier + cross-run merger).
"""

from __future__ import annotations

import json
import logging
import os
import time
from typing import Any, Optional

from bench_core.jsonl import extract_last_json_object
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest

from .prompts import DEDUP_SYSTEM_PROMPT, DEDUP_USER_PROMPT
from .render import render_open_issues_json, render_reports_md

log = logging.getLogger("baml_dedup")

BAML_DEDUP_MODEL = os.environ.get("BAML_DEDUP_MODEL") or os.environ.get("DEDUP_MODEL", "claude-sonnet-4-6")
BAML_DEDUP_MAX_TURNS = int(os.environ.get("BAML_DEDUP_MAX_TURNS") or os.environ.get("DEDUP_MAX_TURNS", "6"))
BAML_DEDUP_TIMEOUT_SECS = int(os.environ.get("BAML_DEDUP_TIMEOUT_SECS") or os.environ.get("DEDUP_TIMEOUT_SECS", "600"))
BATCH_SIZE = int(os.environ.get("BAML_DEDUP_BATCH_SIZE") or os.environ.get("DEDUP_BATCH_SIZE", "20"))
OPEN_STATUSES = {"open", "confirmed", "approved", "fixing"}


class BamlDedup(Processor):
    """Batch queued trophies through the classify/dedup agent and upsert the issues DB."""

    role = "baml-dedup"
    table = "trophies"
    claim_value = "queued"
    claim_into = "deduping"
    lease_ms = 30 * 60 * 1000
    batch = True  # _drain claims one then we gather the rest of the window

    def __init__(self, service):
        """Initialize the processor and build a proxy client from the environment.

        Args:
            service: The backing service handle used for queue claims and DB access.
        """
        super().__init__(service)
        self.proxy = ProxyClient.from_env()

    async def process(self, first: dict[str, Any]) -> None:
        """Dedup a window of queued trophies and upsert the resulting issues.

        Gathers up to BATCH_SIZE queued trophies starting from the already-claimed
        first one, renders them plus the open issues for the agent, runs the agent,
        upserts each skill/language issue it returns, then marks the batch done.

        Args:
            first: The first already-claimed trophy row that seeds the batch.
        """
        # gather a window of queued trophies (first is already claimed)
        batch = [first]
        while len(batch) < BATCH_SIZE:
            more = await self._claim_one()
            if more is None:
                break
            batch.append(more)

        open_issues = await self._open_issues()
        reports_md = render_reports_md(batch)
        open_json = render_open_issues_json(open_issues)

        req = RunAgentRequest(
            cell_id=f"baml-dedup-{int(time.time())}",
            model=BAML_DEDUP_MODEL,
            max_turns=BAML_DEDUP_MAX_TURNS,
            prompt=DEDUP_USER_PROMPT,
            system_prompt=DEDUP_SYSTEM_PROMPT,
            files={"reports.md": reports_md, "open_issues.json": open_json},
            post_file_patterns=["issues.json"],
            max_file_bytes=512 * 1024,
            invocation_timeout_secs=BAML_DEDUP_TIMEOUT_SECS,
        )
        result = await self.proxy.run_agent(req, timeout=BAML_DEDUP_TIMEOUT_SECS + 120)
        items = self._parse_issues(result)
        log.info("baml-dedup batch=%d -> %d issue items", len(batch), len(items))

        for it in items:
            if it.get("kind") not in ("skill", "language"):
                continue
            if not it.get("title"):
                log.warning("baml-dedup: skipping issue item with no title (kind=%s)", it.get("kind"))
                continue
            await self._upsert(it)

        for t in batch:
            await self.service.transition(self.table, t["_id"], "done")

    async def _open_issues(self) -> list[dict[str, Any]]:
        """Fetch the currently-open issues to give the agent dedup context.

        Returns:
            The issue rows whose status is in OPEN_STATUSES.
        """
        rows = await self.service.list("issues", limit=200)
        return [r for r in rows if r.get("status") in OPEN_STATUSES]

    @staticmethod
    def _parse_issues(result) -> list[dict[str, Any]]:
        """Extract the issue list the agent produced from its run result.

        Prefers the posted issues.json file and falls back to the last JSON
        object in the transcript when the file is missing or unparseable.

        Args:
            result: The agent run result carrying post_files and transcript.

        Returns:
            The list of issue dicts, or an empty list when none can be parsed.
        """
        raw = result.post_files.get("issues.json")
        data: Optional[Any] = None
        if raw:
            try:
                data = json.loads(raw)
            except json.JSONDecodeError:
                data = None
        if data is None:
            data = extract_last_json_object(result.transcript or "")
        if isinstance(data, dict):
            return data.get("issues", []) or []
        return []

    async def _upsert(self, it: dict[str, Any]) -> None:
        """Create a new issue or merge evidence into an existing one.

        When the item carries an `id` for an existing issue, merges its evidence
        and refreshes the description/suggestion; otherwise inserts a new issue.

        Args:
            it: A single agent-produced issue item with kind, title, evidence, etc.
        """
        now = int(time.time() * 1000)
        evidence = []
        for e in (it.get("evidence") or []):
            if not e.get("report_id"):
                continue
            entry = {"trophyId": e.get("report_id"), "call_index": e.get("call_index")}
            # Prefer the explicit turn index; only include it when actually present
            # (don't conflate it with call_index, which misleads consumers).
            if e.get("turn_index") is not None:
                entry["turn_index"] = e["turn_index"]
            evidence.append(entry)
        existing_id = it.get("id")
        if existing_id:
            existing = await self.service.get("issues", existing_id)
            if existing:
                merged = self._merge_evidence(existing.get("evidence") or [], evidence)
                patch = {
                    "evidence": merged,
                    "description": it.get("description", existing["description"]),
                    "lastSeenAt": now,
                    "notionSyncStatus": "dirty",
                }
                if it.get("suggestion"):
                    patch["suggestion"] = it["suggestion"]
                await self.service.update("issues", existing_id, patch)
                return
        await self.service.create("issues", {
            "kind": it.get("kind"),
            "title": it.get("title"),
            "description": it.get("description", ""),
            "suggestion": it.get("suggestion"),
            "category": it.get("category") or "bug",
            "evidence": evidence,
            "status": "open",
            "notionSyncStatus": "dirty",
            "firstSeenAt": now,
            "lastSeenAt": now,
        })

    @staticmethod
    def _merge_evidence(old: list[dict], new: list[dict]) -> list[dict]:
        """Append new evidence entries, skipping ones already present.

        Deduplicates on the (trophyId, call_index) pair so re-seen evidence
        does not accumulate.

        Args:
            old: The existing evidence entries to preserve.
            new: The candidate evidence entries to merge in.

        Returns:
            The combined evidence list with duplicates removed.
        """
        seen = {(e.get("trophyId"), e.get("call_index")) for e in old}
        merged = list(old)
        for e in new:
            key = (e.get("trophyId"), e.get("call_index"))
            if key not in seen:
                merged.append(e)
                seen.add(key)
        return merged


if __name__ == "__main__":
    run_processor(BamlDedup)
