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
import uuid
from typing import Any, Optional

from bench_core import cursor_client, slack_client
from bench_core.notion_client import NotionClient
from bench_core.processor import Processor
from bench_core.service_client import ServiceClient

from . import fixer
from .tracker import CursorTracker

log = logging.getLogger("notion_fixer")

NOTION_TOKEN = os.environ.get("ATB_NOTION_TOKEN", "")
NOTION_SKILL_DB_ID = os.environ.get("ATB_NOTION_SKILL_DB_ID", "")
NOTION_LANG_DB_ID = os.environ.get("ATB_NOTION_LANG_DB_ID", "")
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
SLACK_FIX_CHANNEL = os.environ.get("ATB_SLACK_FIX_CHANNEL", "")
# We launch the fix agent through Cursor's Cloud Agents API directly (a Slack
# mention from an app/API can't trigger Cursor). SLACK_BOT_TOKEN is used only for
# the visibility note posted on dispatch, not to trigger anything.
CURSOR_API_KEY = os.environ.get("ATB_CURSOR_API_KEY", "")
CURSOR_MODEL = os.environ.get("CURSOR_MODEL") or None

# Notion board status labels. "not started" is the resting state, "approved"
# and "redraft" are the human-moved triggers, and "to cursor" is set
# automatically when a Cursor fix agent is spawned for the issue.
STATUS_CONFIRMED = os.environ.get("NOTION_STATUS_CONFIRMED", "not started")
STATUS_APPROVED = os.environ.get("NOTION_STATUS_APPROVED", "approved")
STATUS_FIXING = os.environ.get("NOTION_STATUS_FIXING", "to cursor")
# Cursor-tracker-driven board labels (PR phase).
STATUS_PRPREP = os.environ.get("NOTION_STATUS_PRPREP", "pr prep")
STATUS_PR_READY = os.environ.get("NOTION_STATUS_PR_READY", "ready to merge")
STATUS_NEEDS_HUMAN = os.environ.get("NOTION_STATUS_NEEDS_HUMAN", "needs human")
STATUS_MERGED = os.environ.get("NOTION_STATUS_MERGED", "merged")


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
            # Re-render the whole card (title + status + body), so a redrafted
            # rewrite or an updated description/repro/evidence actually shows on
            # the page — not just a status change.
            await self.notion.update_issue_page(
                page_id, issue["title"], self._map_status(issue.get("status", "open")),
                issue.get("description", ""), links,
                suggestion=issue.get("suggestion"), category=issue.get("category"),
                repro=issue.get("repro"), kind=issue.get("kind"),
            )
        else:
            page_id = await self.notion.create_issue_page(
                db_id, issue["title"], STATUS_CONFIRMED, issue.get("description", ""), links,
                suggestion=issue.get("suggestion"), category=issue.get("category"),
                repro=issue.get("repro"), kind=issue.get("kind"),
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
                "approved": STATUS_APPROVED, "dispatching": STATUS_APPROVED,
                "fixing": STATUS_FIXING, "tocursor": STATUS_FIXING,
                "prprep": STATUS_PRPREP, "pr_ready": STATUS_PR_READY,
                "needs_human": STATUS_NEEDS_HUMAN, "closed": STATUS_MERGED,
                }.get(status, STATUS_CONFIRMED)


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
    claim_into = "dispatching"  # transient; -> tocursor on a successful launch
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

        Idempotent without blocking re-dispatch: an issue carrying a real
        ``fixSlackTs`` ref is skipped, but a fresh random ``agentId`` is minted per
        dispatch and persisted as a ``pending:`` marker before launch, so a
        crash-retry resumes the same id (Cursor 409, no duplicate) while a reset
        issue (``fixSlackTs`` cleared) launches a genuinely new agent. A no-op
        (logs a warning) when no Cursor API key is configured. Otherwise launches a
        Cursor cloud agent, records its reference, posts a (non-triggering) Slack
        note linking the agent, and flips the Notion page to fixing.

        Args:
            issue: The claimed issue document.
        """
        issue_id = issue["_id"]
        existing = issue.get("fixSlackTs") or ""
        # A real dispatch ref (anything but a "pending:" marker) means this issue was
        # already handed to Cursor. Don't re-launch; just ensure it lands in tocursor
        # (a crash may have flipped us back to approved before that transition
        # persisted) so the tracker picks it up, and stop.
        if existing and not existing.startswith("pending:"):
            log.info("issue %s already dispatched (ref=%s); ensuring tocursor", issue_id, existing)
            await self.service.transition(
                self.table, issue_id, "tocursor",
                patch={"cursorAgentId": existing, "fixAttempts": issue.get("fixAttempts") or 0},
            )
            return
        if not CURSOR_API_KEY:
            log.warning("no CURSOR_API_KEY configured; cannot dispatch issue %s", issue_id)
            return
        repo = fixer.repo_url(issue["kind"])
        branch = fixer.branch_for(issue["kind"])
        # The Cursor agent id (`bc-<uuid>`) must be STABLE within one dispatch but
        # FRESH across intentional re-dispatches:
        #   * stable so a crash-retry (launch succeeded, fixSlackTs not yet persisted)
        #     reuses the same id and Cursor returns 409 instead of spawning a
        #     duplicate agent/PR;
        #   * fresh so re-approving a reset issue (its fixSlackTs cleared) launches a
        #     NEW agent rather than 409-colliding with the prior, already-used id.
        # Both hold by minting a random id and persisting it as a `pending:` marker
        # BEFORE launching: a retry resumes from the marker, while a reset clears
        # fixSlackTs entirely so the next dispatch mints a new id. (A deterministic
        # id derived from issue_id can never be reused, which silently blocked every
        # legitimate re-dispatch.)
        if existing.startswith("pending:"):
            agent_id = existing[len("pending:"):]
        else:
            agent_id = f"bc-{uuid.uuid4()}"
            await self.service.update(self.table, issue_id, {"fixSlackTs": f"pending:{agent_id}"})
        try:
            result = await cursor_client.launch_agent(
                CURSOR_API_KEY, fixer.cursor_prompt(issue), repo, branch,
                agent_id=agent_id, auto_create_pr=True, model=CURSOR_MODEL,
            )
        except Exception:
            # Launch failed (e.g. Cursor billing 400). Clear the pending marker so the
            # issue isn't left looking dispatched and a later retry mints a clean id.
            await self.service.update(self.table, issue_id, {"fixSlackTs": ""})
            raise
        agent = result.get("agent") or result  # response may nest under "agent"
        ref = agent.get("id") or agent_id
        url = agent.get("url")
        # Visibility note in Slack (a bot post; not a trigger) with the agent link.
        # Skipped on an idempotent re-dispatch to avoid a duplicate note. Its ts
        # becomes the root of the per-issue fix thread: the cursor-tracker posts every
        # later update (PR up, fixing errors, ready to merge, …) as a threaded reply.
        thread_ts: Optional[str] = None
        if not result.get("alreadyLaunched") and SLACK_FIX_CHANNEL and SLACK_BOT_TOKEN:
            link = f"<{url}|view agent>  ·  " if url else ""
            thread_ts = await slack_client.post_message(
                SLACK_BOT_TOKEN, SLACK_FIX_CHANNEL,
                f"Started Cursor agent for *{issue['title']}*\n"
                f"{link}{repo} @ {branch}",
            )
        # Persist the dispatch reference for BOTH a fresh launch and a 409
        # already-launched result, so the guard above short-circuits next time, then
        # move out of the transient `dispatching` state into `tocursor` (releasing the
        # claim so the reaper won't requeue a working fix), recording the thread root.
        # The cursor-tracker owns the issue from here, polling this agent for its PR.
        await self.service.update(self.table, issue_id, {"fixSlackTs": ref})
        await self.service.transition(
            self.table, issue_id, "tocursor",
            patch={"cursorAgentId": ref, "fixAttempts": 0, "fixThreadTs": thread_ts},
        )
        log.info("launched cursor agent %s (%s) for issue %s", ref, url, issue_id)
        if self.notion and issue.get("notionPageId"):
            await self.notion.set_status(issue["notionPageId"], STATUS_FIXING)


async def _amain() -> None:
    """Run the NotionPush + FixDispatch claim loops and the CursorTracker sweep.

    Builds the shared ServiceClient, runs all three concurrently (the two claim loops
    plus the tracker that follows dispatched fix agents through to a mergeable PR),
    and closes the client on exit.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("ATB_SERVICE_TOKEN", ""))
    try:
        await asyncio.gather(
            NotionPush(service).run(),
            FixDispatch(service).run(),
            CursorTracker(service).run(),
        )
    finally:
        await service.aclose()


if __name__ == "__main__":
    asyncio.run(_amain())
