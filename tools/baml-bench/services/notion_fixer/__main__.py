"""notion-sync + fixer: two claim loops over the issues table.

  NotionPush  : claims notionSyncStatus=dirty -> create/patch Notion page -> synced/confirmed
  FixDispatch : claims status=approved        -> launch Cursor cloud agent + Slack note

Both reuse the Processor base (the issues table is itself a claimable queue,
claimed on different fields).
"""

from __future__ import annotations

import asyncio
import logging
import os
from typing import Any, Optional

from bench_core import cursor_client, slack_client
from bench_core.notion_client import NotionClient
from bench_core.processor import Processor
from bench_core.service_client import ServiceClient

from . import fixer

log = logging.getLogger("notion_fixer")

NOTION_TOKEN = os.environ.get("NOTION_TOKEN", "")
NOTION_SKILL_DB_ID = os.environ.get("NOTION_SKILL_DB_ID", "")
NOTION_LANG_DB_ID = os.environ.get("NOTION_LANG_DB_ID", "")
SLACK_BOT_TOKEN = os.environ.get("SLACK_BOT_TOKEN", "")
SLACK_FIX_CHANNEL = os.environ.get("SLACK_FIX_CHANNEL", "")
# We launch the fix agent through Cursor's Cloud Agents API directly (a Slack
# mention from an app/API can't trigger Cursor). SLACK_BOT_TOKEN is used only for
# the visibility note posted on dispatch, not to trigger anything.
CURSOR_API_KEY = os.environ.get("CURSOR_API_KEY", "")
CURSOR_MODEL = os.environ.get("CURSOR_MODEL") or None

STATUS_CONFIRMED = os.environ.get("NOTION_STATUS_CONFIRMED", "confirmed issue")
STATUS_APPROVED = os.environ.get("NOTION_STATUS_APPROVED", "approved issue")
STATUS_FIXING = os.environ.get("NOTION_STATUS_FIXING", "fixing")


class NotionPush(Processor):
    """Claim loop that mirrors dirty issues onto the Notion board.

    Claims issues with ``notionSyncStatus=dirty`` (into ``syncing``), creates or
    patches their Notion page, marks them ``synced``, and promotes a freshly
    boarded ``open`` issue to ``confirmed``. A no-op (marks synced) when Notion is
    not configured for the issue's kind.
    """

    role = "notion-push"
    table = "issues"
    claim_field = "notionSyncStatus"
    claim_index = "by_notion_sync"
    claim_value = "dirty"
    claim_into = "syncing"
    lease_ms = 5 * 60 * 1000

    def __init__(self, service):
        """Build the processor and its Notion client.

        Args:
            service: The ServiceClient used for all Convex reads/writes.
        """
        super().__init__(service)
        self.notion = NotionClient(NOTION_TOKEN) if NOTION_TOKEN else None

    def _db_for(self, kind: str) -> Optional[str]:
        """Return the Notion database id for an issue kind.

        Args:
            kind: The issue kind, ``"skill"`` or anything else (language).

        Returns:
            The configured skill or language database id (may be empty/None).
        """
        return NOTION_SKILL_DB_ID if kind == "skill" else NOTION_LANG_DB_ID

    async def process(self, issue: dict[str, Any]) -> None:
        """Sync one claimed issue to Notion and mark it synced.

        Creates the page (when the issue has no ``notionPageId``) or patches its
        status, records the page id, flips ``notionSyncStatus`` to ``synced``, and
        confirms the issue. Marks synced without touching Notion when the kind has
        no configured database.

        Args:
            issue: The claimed issue document.
        """
        issue_id = issue["_id"]
        db_id = self._db_for(issue["kind"])
        if not self.notion or not db_id:
            log.info("notion not configured for kind=%s; marking synced (no-op)", issue["kind"])
            await self.service.update(self.table, issue_id,
                                      {"notionSyncStatus": "synced"}, )
            await self._confirm(issue_id, issue)
            return

        links = fixer.evidence_links(issue)
        page_id = issue.get("notionPageId")
        if page_id:
            await self.notion.set_status(page_id, self._map_status(issue.get("status", "open")))
        else:
            page_id = await self.notion.create_issue_page(
                db_id, issue["title"], STATUS_CONFIRMED, issue.get("description", ""), links,
                suggestion=issue.get("suggestion"), category=issue.get("category"),
            )
        await self.service.update(self.table, issue_id,
                                  {"notionPageId": page_id, "notionSyncStatus": "synced"})
        await self._confirm(issue_id, issue)

    async def _confirm(self, issue_id: str, issue: dict[str, Any]) -> None:
        """Promote a just-boarded ``open`` issue to ``confirmed``.

        Only ``open`` issues are promoted, so later lifecycle states are never
        downgraded. The claim is kept (``release_claim=False``) so ``process`` can
        finish its work.

        Args:
            issue_id: The issue's Convex id.
            issue: The issue document (its ``status`` is inspected).
        """
        # promote open -> confirmed once it's on the board (don't downgrade later states)
        if issue.get("status") == "open":
            await self.service.transition(self.table, issue_id, "confirmed", release_claim=False)

    @staticmethod
    def _map_status(status: str) -> str:
        """Map an internal issue status to its Notion board status label.

        Args:
            status: The internal status (``open``/``confirmed``/``approved``/``fixing``).

        Returns:
            The configured Notion status label, defaulting to the confirmed label
            for any unrecognized status.
        """
        return {"open": STATUS_CONFIRMED, "confirmed": STATUS_CONFIRMED,
                "approved": STATUS_APPROVED, "fixing": STATUS_FIXING}.get(status, STATUS_CONFIRMED)


class FixDispatch(Processor):
    """Claim loop that dispatches approved issues to Cursor for a fix.

    Claims issues with ``status=approved`` (into ``fixing``), launches a Cursor
    cloud agent via the API, records the agent reference on the issue, posts a
    Slack visibility note linking the agent, and flips its Notion page to the
    fixing status. A no-op when ``CURSOR_API_KEY`` is not configured.
    """

    role = "fix-dispatch"
    table = "issues"
    claim_field = "status"
    claim_index = "by_status_created"
    claim_value = "approved"
    claim_into = "fixing"
    lease_ms = 5 * 60 * 1000

    def __init__(self, service):
        """Build the processor and its Notion client.

        Args:
            service: The ServiceClient used for all Convex reads/writes.
        """
        super().__init__(service)
        self.notion = NotionClient(NOTION_TOKEN) if NOTION_TOKEN else None

    async def process(self, issue: dict[str, Any]) -> None:
        """Dispatch a fix for one claimed approved issue.

        Idempotent: an issue that already carries ``fixSlackTs`` is skipped, and
        the launch uses a stable ``agentId`` derived from the issue id so a
        re-dispatch after a crash (before ``fixSlackTs`` was persisted) returns
        409 and is treated as already launched rather than spawning a duplicate.
        A no-op (logs a warning) when no Cursor API key is configured. Otherwise
        launches a Cursor cloud agent, records its reference, posts a
        (non-triggering) Slack note linking the agent, and flips the Notion page
        to fixing.

        Args:
            issue: The claimed issue document.
        """
        issue_id = issue["_id"]
        if issue.get("fixSlackTs"):
            log.info("issue %s already dispatched (ref=%s)", issue_id, issue["fixSlackTs"])
            return
        if not CURSOR_API_KEY:
            log.warning("no CURSOR_API_KEY configured; cannot dispatch issue %s", issue_id)
            return
        repo = fixer.repo_url(issue["kind"])
        # Launch a Cursor cloud agent directly (the real automated path). The
        # agentId is derived from the issue id so a re-dispatch after a crash
        # (before fixSlackTs was persisted) is idempotent: Cursor returns 409 and
        # launch_agent reports it as already launched instead of spawning a
        # duplicate agent/PR.
        agent_id = f"notion-fixer-{issue_id}"
        result = await cursor_client.launch_agent(
            CURSOR_API_KEY, fixer.cursor_prompt(issue), repo, fixer.CURSOR_BRANCH,
            agent_id=agent_id, auto_create_pr=True, model=CURSOR_MODEL,
        )
        agent = result.get("agent") or result  # response may nest under "agent"
        ref = agent.get("id") or agent_id
        url = agent.get("url")
        # Persist the dispatch reference for BOTH a fresh launch and a 409
        # already-launched result, so the guard above short-circuits next time.
        await self.service.update(self.table, issue_id, {"fixSlackTs": ref})
        # Visibility note in Slack (a bot post; not a trigger) with the agent link.
        # Skipped on an idempotent re-dispatch to avoid a duplicate note.
        if not result.get("alreadyLaunched") and SLACK_FIX_CHANNEL and SLACK_BOT_TOKEN:
            link = f"<{url}|view agent>  ·  " if url else ""
            await slack_client.post_message(
                SLACK_BOT_TOKEN, SLACK_FIX_CHANNEL,
                f"Started Cursor agent for *{issue['title']}*\n"
                f"{link}{repo} @ {fixer.CURSOR_BRANCH}",
            )
        log.info("launched cursor agent %s (%s) for issue %s", ref, url, issue_id)
        if self.notion and issue.get("notionPageId"):
            await self.notion.set_status(issue["notionPageId"], STATUS_FIXING)


async def _amain() -> None:
    """Run the NotionPush and FixDispatch claim loops together until cancelled.

    Builds the shared ServiceClient, runs both processors concurrently, and closes
    the client on exit.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("SERVICE_TOKEN", ""))
    try:
        await asyncio.gather(NotionPush(service).run(), FixDispatch(service).run())
    finally:
        await service.aclose()


if __name__ == "__main__":
    asyncio.run(_amain())
