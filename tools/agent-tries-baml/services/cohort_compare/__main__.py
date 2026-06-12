"""Cohort-compare processor: claim a ready skill-arena cohort, run the compare agent
over its member runs, and emit a single comparison "cohort trophy" that flows into
dedup like any other trophy.

Inherits the same Processor base as every other stage (the cohorts table is a
claimable queue); it is structurally a sibling of baml_dedup — render member runs,
run one agent, parse its JSON, write the result — but its unit of work is a cohort,
not a batch of trophies.
"""

from __future__ import annotations

import json
import logging
import os
from typing import Any, Optional

from bench_core import slack_client
from bench_core.jsonl import extract_last_json_object
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest

from .prompts import ARENA_SYSTEM_PROMPT, ARENA_USER_PROMPT
from .render import render_arena_md

log = logging.getLogger("cohort_compare")

ARENA_MODEL = os.environ.get("ARENA_MODEL", "claude-sonnet-4-6")
ARENA_MAX_TURNS = int(os.environ.get("ARENA_MAX_TURNS", "8"))
ARENA_TIMEOUT_SECS = int(os.environ.get("ARENA_TIMEOUT_SECS", "600"))
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://bench3-ui.fly.dev")


class CohortCompare(Processor):
    """Claim a queued cohort, compare its variant runs, and emit the cohort trophy."""

    role = "cohort-compare"
    table = "cohorts"
    claim_value = "queued"
    claim_into = "comparing"
    lease_ms = 30 * 60 * 1000

    def __init__(self, service):
        """Bind the service client and build a proxy client from the environment.

        Args:
            service: Client used for all claim, transition, and persistence calls.
        """
        super().__init__(service)
        self.proxy = ProxyClient.from_env()

    async def process(self, cohort: dict[str, Any]) -> None:
        """Compare one cohort's member runs and persist the cohort trophy.

        Gathers each member task's held trophy, renders the arena doc, runs the
        compare agent, creates a cohort-report trophy (status ``queued`` so dedup
        picks it up), releases the held member trophies to ``done``, posts a Slack
        comparison summary, and finishes the cohort.

        Args:
            cohort: The claimed cohort document.
        """
        cohort_id = cohort["_id"]
        members = await self.service.list("tasks", field="cohortId", value=cohort_id,
                                          index="by_cohort")
        # Pair each member task with its held (cohort_member) trophy, if any.
        variants: list[tuple[dict[str, Any], Optional[dict[str, Any]]]] = []
        for m in members:
            trophies = await self.service.list("trophies", field="taskId", value=m["_id"],
                                               index="by_task")
            variants.append((m, trophies[0] if trophies else None))

        doc = render_arena_md(cohort, variants)
        req = RunAgentRequest(
            cell_id=f"cohort-compare-{cohort_id}",
            model=ARENA_MODEL,
            max_turns=ARENA_MAX_TURNS,
            prompt=ARENA_USER_PROMPT,
            system_prompt=ARENA_SYSTEM_PROMPT,
            files={"arena.md": doc},
            post_file_patterns=["comparison.json"],
            max_file_bytes=512 * 1024,
            invocation_timeout_secs=ARENA_TIMEOUT_SECS,
        )
        result = await self.proxy.run_agent(req, timeout=ARENA_TIMEOUT_SECS + 120)
        comp = self._parse_comparison(result)

        # The cohort trophy isn't authored by one task; trophies.taskId is required,
        # so anchor it to a representative member (keeps by_task / dashboard links
        # resolving) and mark it a cohort report via isCohortReport + cohortId.
        rep_task_id = members[0]["_id"] if members else cohort_id
        findings = [
            {
                "kind": f.get("kind"),
                "title": f.get("title"),
                "description": f.get("description"),
                "anchor": {"call_index": f.get("call_index"), "turn_index": f.get("turn_index")},
                "suggestion": f.get("suggestion"),
            }
            for f in (comp.get("findings") or [])
            if f.get("kind") in ("skill", "language") and f.get("title")
        ]
        trophy = {
            "taskId": rep_task_id,
            "outcome": "success",
            "isCohortReport": True,
            "cohortId": cohort_id,
            "metrics": {},
            "summary": comp.get("summary"),
            "whatWentWell": comp.get("what_went_well") or [],
            "whatFailed": comp.get("what_failed") or [],
            "reportMd": comp.get("report_md") or doc,
            "findings": findings,
            "suggestions": comp.get("suggestions") or [],
            "status": "queued",  # enters dedup like any other trophy
        }
        trophy_id = await self.service.create("trophies", trophy)

        # Release the held member trophies now that the comparison has consumed them.
        for _, tr in variants:
            if tr is not None:
                await self.service.transition("trophies", tr["_id"], "done")

        await self.service.transition("cohorts", cohort_id, "done",
                                      patch={"reportTrophyId": trophy_id})
        log.info("cohort %s -> trophy %s (%d variants, %d findings)",
                 cohort_id, trophy_id, len(variants), len(findings))
        await self._notify(cohort, comp, trophy_id)

    @staticmethod
    def _parse_comparison(result) -> dict[str, Any]:
        """Extract the comparison object the agent produced from its run result.

        Prefers the posted comparison.json file and falls back to the last JSON
        object in the transcript when the file is missing or unparseable.

        Args:
            result: The agent run result carrying post_files and transcript.

        Returns:
            The parsed comparison dict, or an empty dict when none can be recovered.
        """
        raw = result.post_files.get("comparison.json")
        if raw:
            try:
                return json.loads(raw)
            except json.JSONDecodeError:
                pass
        scraped = extract_last_json_object(result.transcript or "")
        return scraped if isinstance(scraped, dict) else {}

    async def _notify(self, cohort: dict[str, Any], comp: dict[str, Any],
                      trophy_id: str) -> None:
        """Post the arena comparison summary to the cohort's Slack thread.

        No-ops when the cohort has no Slack channel or no bot token is configured.

        Args:
            cohort: The cohort document (provides Slack channel and thread).
            comp: The parsed comparison (provides the summary line).
            trophy_id: Id of the created cohort trophy, used to build the link.
        """
        channel = cohort.get("slackChannel")
        if not channel or not SLACK_BOT_TOKEN:
            return
        branches = ", ".join(cohort.get("skillRefs") or [])
        summary = comp.get("summary") or "Arena complete."
        link = f"{UI_BASE_URL.rstrip('/')}/cohorts/{cohort['_id']}"
        text = (f"*Skill arena complete.* {summary}\n"
                f"variants: {branches}\n"
                f"<{link}|view comparison>")
        await slack_client.post_message(
            SLACK_BOT_TOKEN, channel, text, thread_ts=cohort.get("slackThreadTs"),
        )


if __name__ == "__main__":
    run_processor(CohortCompare)
