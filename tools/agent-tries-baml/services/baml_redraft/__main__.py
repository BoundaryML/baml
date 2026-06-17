"""BAML redraft processor: a reviewer moves an issue to the 'redraft' status on
the Linear board and leaves comments; this claims the issue, pulls those comments
as feedback, runs an agent to rewrite the issue, and pushes it back to the board
(linearSyncStatus=dirty, status=confirmed) for another human review pass.
"""

from __future__ import annotations

import json
import logging
import os
import time
from typing import Any, Optional

from bench_core.jsonl import extract_last_json_object
from bench_core.linear_client import LinearClient
from bench_core.processor import Processor, run_processor
from bench_core.proxy_client import ProxyClient
from bench_core.schemas import RunAgentRequest

from .prompts import REDRAFT_SYSTEM_PROMPT, REDRAFT_USER_PROMPT

log = logging.getLogger("baml_redraft")

BAML_REDRAFT_MODEL = os.environ.get("BAML_REDRAFT_MODEL") or os.environ.get("DEDUP_MODEL", "claude-sonnet-4-6")
BAML_REDRAFT_MAX_TURNS = int(os.environ.get("BAML_REDRAFT_MAX_TURNS", "6"))
BAML_REDRAFT_TIMEOUT_SECS = int(os.environ.get("BAML_REDRAFT_TIMEOUT_SECS", "600"))
LINEAR_API_KEY = os.environ.get("ATB_LINEAR_TOKEN", "")


def render_feedback(comments: list[dict[str, Any]]) -> str:
    """Render Linear comments into the feedback.md fed to the agent.

    Args:
        comments: The issue comments ({text, created_time, author}).

    Returns:
        A Markdown bullet list of the non-empty comment texts.
    """
    lines = ["# Reviewer feedback (from the Linear board)", ""]
    for c in comments:
        text = (c.get("text") or "").strip()
        if text:
            lines.append(f"- {text}")
    return "\n".join(lines)


def render_issue_json(issue: dict[str, Any]) -> str:
    """Render the current issue into the issue.json fed to the agent.

    Args:
        issue: The issue row being redrafted.

    Returns:
        A pretty-printed JSON object of the agent-relevant issue fields.
    """
    return json.dumps({
        "title": issue.get("title"),
        "kind": issue.get("kind"),
        "category": issue.get("category"),
        "description": issue.get("description"),
        "suggestion": issue.get("suggestion"),
        "repro": issue.get("repro"),
    }, indent=2)


class BamlRedraft(Processor):
    """Claim redraft-flagged issues, rewrite them from reviewer feedback, re-board them."""

    role = "baml-redraft"
    table = "issues"
    claim_field = "status"
    claim_value = "redraft"
    claim_into = "redrafting"
    lease_ms = 30 * 60 * 1000

    def __init__(self, service):
        """Initialize the processor with a proxy client and a Linear client.

        Args:
            service: The backing service handle for queue claims and DB access.
        """
        super().__init__(service)
        self.proxy = ProxyClient.from_env()
        self.linear = LinearClient(LINEAR_API_KEY) if LINEAR_API_KEY else None

    async def process(self, issue: dict[str, Any]) -> None:
        """Redraft one issue from its Linear comments and return it to the board.

        Pulls the issue comments as feedback, runs the redraft agent, applies the
        rewritten title/description/suggestion, marks the issue dirty so LinearPush
        re-syncs it, and transitions it back to ``confirmed`` for another review.
        With no feedback (or no Linear configured) it just re-boards the issue.

        Args:
            issue: The claimed issue document (status was ``redraft``).
        """
        issue_id = issue["_id"]
        comments = await self._comments(issue.get("linearIssueId"))
        if not comments:
            log.info("baml-redraft: no feedback for issue %s; returning to board", issue_id)
            await self.service.transition("issues", issue_id, "confirmed", field="status")
            return

        req = RunAgentRequest(
            cell_id=f"baml-redraft-{issue_id}-{int(time.time())}",
            model=BAML_REDRAFT_MODEL,
            max_turns=BAML_REDRAFT_MAX_TURNS,
            prompt=REDRAFT_USER_PROMPT,
            system_prompt=REDRAFT_SYSTEM_PROMPT,
            files={"issue.json": render_issue_json(issue), "feedback.md": render_feedback(comments)},
            post_file_patterns=["issue.json"],
            max_file_bytes=512 * 1024,
            invocation_timeout_secs=BAML_REDRAFT_TIMEOUT_SECS,
        )
        result = await self.proxy.run_agent(req, timeout=BAML_REDRAFT_TIMEOUT_SECS + 120)
        new = self._parse_issue(result)

        patch: dict[str, Any] = {"lastSeenAt": int(time.time() * 1000), "linearSyncStatus": "dirty"}
        for field in ("title", "description", "suggestion"):
            if new.get(field):
                patch[field] = new[field]
        await self.service.update("issues", issue_id, patch)
        # Back to the board for another human review pass (LinearPush re-syncs the
        # dirty issue; status=confirmed maps to the not-started Linear label).
        await self.service.transition("issues", issue_id, "confirmed", field="status")
        log.info("baml-redraft: issue %s redrafted from %d comment(s)", issue_id, len(comments))

    async def _comments(self, linear_issue_id: Optional[str]) -> list[dict[str, Any]]:
        """Fetch the Linear comments for an issue, tolerating failures.

        Args:
            linear_issue_id: The issue's Linear id, or None when it has no card.

        Returns:
            The issue comments, or an empty list when unavailable.
        """
        if not self.linear or not linear_issue_id:
            return []
        try:
            return await self.linear.get_comments(linear_issue_id)
        except Exception:  # noqa: BLE001
            log.exception("baml-redraft: failed to read comments for issue %s", linear_issue_id)
            return []

    @staticmethod
    def _parse_issue(result) -> dict[str, Any]:
        """Extract the rewritten issue object the agent produced.

        Prefers the posted issue.json file and falls back to the last JSON object
        in the transcript.

        Args:
            result: The agent run result carrying post_files and transcript.

        Returns:
            The parsed issue dict, or an empty dict when none can be recovered.
        """
        raw = result.post_files.get("issue.json")
        if raw:
            try:
                data = json.loads(raw)
                if isinstance(data, dict):
                    return data
            except json.JSONDecodeError:
                pass
        scraped = extract_last_json_object(result.transcript or "")
        return scraped if isinstance(scraped, dict) else {}


if __name__ == "__main__":
    run_processor(BamlRedraft)
