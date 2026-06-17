"""linear-sync + fixer: two claim loops over the issues table.

  LinearPush  : claims linearSyncStatus=dirty -> create/adopt/update Linear issue -> synced/confirmed
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
from bench_core import linear_client as lc
from bench_core.linear_client import LinearClient
from bench_core.processor import Processor
from bench_core.service_client import ServiceClient

from . import fixer
from .tracker import CursorTracker

log = logging.getLogger("notion_fixer")

LINEAR_API_KEY = os.environ.get("ATB_LINEAR_TOKEN", "")
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
SLACK_FIX_CHANNEL = os.environ.get("ATB_SLACK_FIX_CHANNEL", "")
# We launch the fix agent through Cursor's Cloud Agents API directly (a Slack
# mention from an app/API can't trigger Cursor). SLACK_BOT_TOKEN is used only for
# the visibility note posted on dispatch, not to trigger anything.
CURSOR_API_KEY = os.environ.get("ATB_CURSOR_API_KEY", "")
CURSOR_MODEL = os.environ.get("CURSOR_MODEL") or None

# Linear status-group label ids. not-started is the resting state, approved /
# redraft are the human-moved triggers, and to-cursor is set automatically when a
# Cursor fix agent is spawned for the issue. (env-overridable via linear_client.)
STATUS_CONFIRMED = lc.LINEAR_STATUS_NOT_STARTED
STATUS_APPROVED = lc.LINEAR_STATUS_APPROVED
STATUS_FIXING = lc.LINEAR_STATUS_TO_CURSOR


class LinearPush(Processor):
    """Claim loop that mirrors dirty issues onto the Linear board (1:1 with Convex).

    Claims issues with ``linearSyncStatus=dirty`` (into ``syncing``), then either
    adopts an already-imported card by exact title, creates a new card, or
    re-renders the existing one; records the Linear issue id, marks the row
    ``synced``, and promotes a freshly boarded ``open`` issue to ``confirmed``. A
    no-op (marks synced) when no Linear API key is configured.
    """

    role = "linear-push"
    table = "issues"
    claim_field = "linearSyncStatus"
    claim_index = "by_linear_sync"
    claim_value = "dirty"
    claim_into = "syncing"
    lease_ms = 5 * 60 * 1000

    def __init__(self, service):
        """Build the processor and its Linear client.

        Args:
            service: The ServiceClient used for all Convex reads/writes.
        """
        super().__init__(service)
        self.linear = LinearClient(LINEAR_API_KEY) if LINEAR_API_KEY else None

    async def process(self, issue: dict[str, Any]) -> None:
        """Sync one claimed issue to Linear and mark it synced.

        Resolves the target Linear issue (the stored id, else an adopt-by-title
        match, else a fresh create), re-renders its title/status/body, records the
        id, flips ``linearSyncStatus`` to ``synced``, and confirms the issue.
        Marks synced without touching Linear when no API key is configured.

        Args:
            issue: The claimed issue document.
        """
        issue_id = issue["_id"]
        if not self.linear:
            log.info("linear not configured; marking synced (no-op)")
            await self.service.update(self.table, issue_id, {"linearSyncStatus": "synced"})
            await self._confirm(issue_id, issue)
            return

        links = fixer.evidence_links(issue)
        issue_link = fixer.issue_link(issue)
        pr_url = issue.get("prUrl")
        status_label = self._map_status(issue.get("status", "open"))
        linear_id = issue.get("linearIssueId")
        # Adopt an already-imported card by exact title before creating a new one,
        # so the one-time backfill doesn't duplicate the 100 hand-imported cards.
        if not linear_id:
            linear_id = await self.linear.find_issue_by_title(issue["title"])
        if linear_id:
            # Re-render the whole card (title + status + body), so a redrafted
            # rewrite or an updated description/repro/evidence actually shows —
            # not just a status change.
            await self.linear.update_issue(
                linear_id, issue["title"], status_label,
                issue.get("description", ""), links,
                suggestion=issue.get("suggestion"), category=issue.get("category"),
                repro=issue.get("repro"), issue_link=issue_link, pr_url=pr_url,
            )
        else:
            linear_id = await self.linear.create_issue(
                issue["title"], status_label, issue.get("description", ""), links,
                suggestion=issue.get("suggestion"), category=issue.get("category"),
                repro=issue.get("repro"), issue_link=issue_link, pr_url=pr_url,
            )
        await self.service.update(self.table, issue_id,
                                  {"linearIssueId": linear_id, "linearSyncStatus": "synced"})
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
        """Map an internal issue status to its Linear status-group label id.

        ``redraft`` maps to the redraft label so a re-render mid-redraft doesn't
        clobber the human's label back to not-started.

        Args:
            status: The internal status (``open``/``confirmed``/``approved``/…).

        Returns:
            The Linear status-group label id, defaulting to the not-started label
            for any unrecognized status.
        """
        return {"open": STATUS_CONFIRMED, "confirmed": STATUS_CONFIRMED,
                "approved": STATUS_APPROVED, "dispatching": STATUS_APPROVED,
                "fixing": STATUS_FIXING, "tocursor": STATUS_FIXING,
                "prprep": lc.LINEAR_STATUS_PR_PREP, "pr_ready": lc.LINEAR_STATUS_READY_TO_MERGE,
                "needs_human": lc.LINEAR_STATUS_NEEDS_HUMAN, "closed": lc.LINEAR_STATUS_MERGED,
                "redraft": lc.LINEAR_STATUS_REDRAFT, "redrafting": lc.LINEAR_STATUS_REDRAFT,
                }.get(status, STATUS_CONFIRMED)


class FixDispatch(Processor):
    """Claim loop that dispatches approved issues to Cursor for a fix.

    Claims issues with ``status=approved`` (into ``fixing``), launches a Cursor
    cloud agent via the API, records the agent reference on the issue, posts a
    Slack visibility note linking the agent, and flips its Linear card to the
    to-cursor status. A no-op when ``CURSOR_API_KEY`` is not configured.
    """

    role = "fix-dispatch"
    table = "issues"
    claim_field = "status"
    claim_index = "by_status_created"
    claim_value = "approved"
    claim_into = "dispatching"  # transient; -> tocursor on a successful launch
    lease_ms = 5 * 60 * 1000

    def __init__(self, service):
        """Build the processor and its Linear client.

        Args:
            service: The ServiceClient used for all Convex reads/writes.
        """
        super().__init__(service)
        self.linear = LinearClient(LINEAR_API_KEY) if LINEAR_API_KEY else None

    async def process(self, issue: dict[str, Any]) -> None:
        """Dispatch a fix for one claimed approved issue.

        Idempotent without blocking re-dispatch: an issue carrying a real
        ``fixSlackTs`` ref is skipped, but a fresh random ``agentId`` is minted per
        dispatch and persisted as a ``pending:`` marker before launch, so a
        crash-retry resumes the same id (Cursor 409, no duplicate) while a reset
        issue (``fixSlackTs`` cleared) launches a genuinely new agent. A no-op
        (logs a warning) when no Cursor API key is configured. Otherwise launches a
        Cursor cloud agent, records its reference, posts a (non-triggering) Slack
        note linking the agent, and flips the Linear card to to-cursor.

        Args:
            issue: The claimed issue document.
        """
        issue_id = issue["_id"]
        if not CURSOR_API_KEY:
            log.warning("no CURSOR_API_KEY configured; cannot dispatch issue %s", issue_id)
            return
        existing = issue.get("fixSlackTs") or ""
        # A real dispatch ref (not a "pending:" marker) means this issue was already
        # handed to Cursor. If that agent is still working — or already opened a PR —
        # don't launch a duplicate: just ensure tocursor so the tracker carries it on.
        # But if the prior agent is DEAD (terminal/expired with no PR, or unreachable),
        # this claim is a human RE-APPROVAL of a stalled fix, so fall through and launch
        # a genuinely fresh agent (clearing the stale ref).
        if existing and not existing.startswith("pending:"):
            if await self._prior_dispatch_alive(existing):
                log.info("issue %s already dispatched (ref=%s, still live); ensuring tocursor",
                         issue_id, existing)
                await self.service.transition(
                    self.table, issue_id, "tocursor",
                    patch={"cursorAgentId": existing, "fixAttempts": issue.get("fixAttempts") or 0},
                )
                return
            log.info("issue %s prior agent %s is dead with no PR; re-dispatching fresh",
                     issue_id, existing)
            existing = ""
            await self.service.update(self.table, issue_id, {"fixSlackTs": ""})
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
        # Best-effort board update — a Linear error (e.g. a transient API failure)
        # must NOT fail the dispatch and orphan a successfully-launched agent.
        if self.linear and issue.get("linearIssueId"):
            try:
                await self.linear.set_status(issue["linearIssueId"], STATUS_FIXING)
            except Exception:  # noqa: BLE001
                log.warning("linear set_status failed for %s; dispatch still succeeded", issue_id)

    @staticmethod
    async def _prior_dispatch_alive(ref: str) -> bool:
        """Whether a prior fix agent is still worth keeping (vs. re-dispatching).

        Returns True when the agent is still working OR has already opened a PR (so
        a fresh dispatch would just duplicate it), and False when it is terminal/
        expired with no PR — or unreachable — meaning a human re-approval should
        launch a brand-new agent.

        Args:
            ref: The existing Cursor agent id stored on the issue.

        Returns:
            True to keep the existing dispatch; False to launch fresh.
        """
        try:
            pr = await cursor_client.pr_for_agent(CURSOR_API_KEY, ref)
        except Exception:  # noqa: BLE001
            log.info("prior agent %s unreachable; treating as dead", ref)
            return False
        if pr is None:
            return False
        if pr.get("prUrl"):
            return True  # already opened a PR; the tracker carries it to prprep
        terminal = (pr.get("runStatus") in cursor_client.TERMINAL_RUN_STATUSES
                    or pr.get("agentStatus") in cursor_client.TERMINAL_RUN_STATUSES)
        return not terminal


async def _amain() -> None:
    """Run the LinearPush + FixDispatch claim loops and the CursorTracker sweep.

    Builds the shared ServiceClient, runs all three concurrently (the two claim loops
    plus the tracker that follows dispatched fix agents through to a mergeable PR),
    and closes the client on exit.
    """
    logging.basicConfig(level=os.environ.get("LOG_LEVEL", "INFO"))
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("ATB_SERVICE_TOKEN", ""))
    try:
        await asyncio.gather(
            LinearPush(service).run(),
            FixDispatch(service).run(),
            CursorTracker(service).run(),
        )
    finally:
        await service.aclose()


if __name__ == "__main__":
    asyncio.run(_amain())
