"""Cursor PR tracker — a sweep that follows a dispatched fix agent to a mergeable PR.

After FixDispatch hands an issue to a Cursor agent (status ``tocursor``), this sweep
watches it through:

  tocursor  -- poll the agent until it opens a PR -> prprep (or no PR after the run
               finishes -> needs_human)
  prprep    -- read the PR's CI + CodeRabbit state:
                 * merged                         -> closed
                 * CI failing OR CodeRabbit blocks -> dispatch a fix (new agent by
                   default, or a follow-up run), up to CURSOR_MAX_FIX_ATTEMPTS, then
                   -> needs_human; Slack-notify each fix
                 * CI passing AND CodeRabbit clear -> pr_ready (a human merges); Slack
                 * otherwise (checks pending / no review yet) -> wait

It is a periodic sweep (not a claim loop), mirroring the cohort reconciler
(services/cron/reconcile.py): idempotent, keyed off the issue ``status``, using only
generic table verbs so it behaves identically on the real and in-memory backends.
"""

from __future__ import annotations

import asyncio
import logging
import os
import uuid
from typing import Any, Optional

from bench_core import cursor_client, github_client, slack_client
from bench_core.notion_client import NotionClient

log = logging.getLogger("cursor_tracker")

CURSOR_API_KEY = os.environ.get("ATB_CURSOR_API_KEY", "")
CURSOR_MODEL = os.environ.get("CURSOR_MODEL") or None
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
SLACK_FIX_CHANNEL = os.environ.get("ATB_SLACK_FIX_CHANNEL", "")
NOTION_TOKEN = os.environ.get("ATB_NOTION_TOKEN", "")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://bench3-ui.fly.dev")

TRACK_SECS = int(os.environ.get("CURSOR_TRACK_SECS", "60"))
# new_agent (launch a fresh agent from the PR branch each fix) | followup (POST a new
# run to the same agent, keeping its branch/PR).
REFIX_MODE = os.environ.get("CURSOR_REFIX_MODE", "new_agent")
MAX_FIX_ATTEMPTS = int(os.environ.get("CURSOR_MAX_FIX_ATTEMPTS", "3"))
TRACK_LIMIT = int(os.environ.get("CURSOR_TRACK_LIMIT", "100"))

# Notion board labels for the tracker-driven statuses.
N_PRPREP = os.environ.get("NOTION_STATUS_PRPREP", "pr prep")
N_PR_READY = os.environ.get("NOTION_STATUS_PR_READY", "ready to merge")
N_NEEDS_HUMAN = os.environ.get("NOTION_STATUS_NEEDS_HUMAN", "needs human")
N_MERGED = os.environ.get("NOTION_STATUS_MERGED", "merged")


class CursorTracker:
    """Sweep that drives dispatched fix agents through tocursor -> prprep -> pr_ready."""

    def __init__(self, service):
        """Bind the service client and build a Notion client for board updates.

        Args:
            service: The ServiceClient used for all reads/writes.
        """
        self.service = service
        self.notion = NotionClient(NOTION_TOKEN) if NOTION_TOKEN else None

    async def run(self) -> None:
        """Run the sweep forever on the ``CURSOR_TRACK_SECS`` cadence.

        A transient sweep failure is logged and retried next tick rather than
        killing the loop.
        """
        log.info("cursor-tracker starting (every %ss, refix=%s, cap=%d)",
                 TRACK_SECS, REFIX_MODE, MAX_FIX_ATTEMPTS)
        while True:
            try:
                await self.sweep_once()
            except Exception:  # noqa: BLE001
                log.exception("cursor-tracker sweep failed")
            await asyncio.sleep(TRACK_SECS)

    async def sweep_once(self) -> None:
        """Track every issue currently in ``tocursor`` or ``prprep`` once.

        A no-op when no Cursor API key is configured. Per-issue failures are logged
        and skipped so one bad PR can't stall the rest.
        """
        if not CURSOR_API_KEY:
            return
        # "fixing" is the legacy pre-tracker dispatched state; sweep it like tocursor
        # so any issue already in flight at deploy time isn't orphaned.
        for status in ("tocursor", "fixing", "prprep"):
            rows = await self.service.list("issues", field="status", value=status,
                                           index="by_status_created", limit=TRACK_LIMIT)
            for issue in rows:
                try:
                    await self._track_one(issue)
                except Exception:  # noqa: BLE001
                    log.exception("cursor-tracker: track failed for issue %s", issue.get("_id"))

    async def _track_one(self, issue: dict[str, Any]) -> None:
        """Advance one in-flight issue based on its status.

        Args:
            issue: The issue row (status is ``tocursor`` or ``prprep``).
        """
        if issue.get("status") in ("tocursor", "fixing"):
            await self._track_tocursor(issue)
        else:
            await self._track_prprep(issue)

    @staticmethod
    def _agent_id(issue: dict[str, Any]) -> Optional[str]:
        """Return the working Cursor agent id for an issue, or None if not real yet.

        Prefers ``cursorAgentId``; falls back to the legacy ``fixSlackTs`` ref. A
        ``pending:`` marker (dispatch not yet confirmed) returns None.

        Args:
            issue: The issue row.

        Returns:
            The agent id, or None.
        """
        aid = issue.get("cursorAgentId") or issue.get("fixSlackTs") or ""
        return None if (not aid or aid.startswith("pending:")) else aid

    async def _track_tocursor(self, issue: dict[str, Any]) -> None:
        """Poll a working agent: advance to prprep on a PR, or escalate if it gave up.

        Args:
            issue: The ``tocursor`` issue row.
        """
        issue_id = issue["_id"]
        agent_id = self._agent_id(issue)
        if not agent_id:
            return
        pr = await cursor_client.pr_for_agent(CURSOR_API_KEY, agent_id)
        if pr is None:
            return
        if pr.get("prUrl"):
            patch = {"prUrl": pr["prUrl"], "prBranch": pr.get("branch")}
            parsed = github_client.parse_pr_url(pr["prUrl"])
            if parsed:
                patch["prNumber"] = parsed[2]
            await self.service.transition("issues", issue_id, "prprep", patch=patch)
            await self._set_notion(issue, N_PRPREP)
            await self._notify(issue, f"PR up for *{issue.get('title')}*\n{pr['prUrl']}")
            log.info("issue %s tocursor -> prprep (%s)", issue_id, pr["prUrl"])
            return
        # No PR. If the agent's run is over and it never opened one, escalate.
        if pr.get("runStatus") in cursor_client.TERMINAL_RUN_STATUSES:
            await self.service.transition("issues", issue_id, "needs_human")
            await self._set_notion(issue, N_NEEDS_HUMAN)
            await self._notify(
                issue,
                f"Cursor agent finished with no PR for *{issue.get('title')}* — needs a human.")
            log.info("issue %s tocursor -> needs_human (no PR)", issue_id)

    async def _track_prprep(self, issue: dict[str, Any]) -> None:
        """Read the PR's CI + CodeRabbit state and advance / fix / escalate.

        Args:
            issue: The ``prprep`` issue row.
        """
        issue_id = issue["_id"]
        agent_id = self._agent_id(issue)
        pr_url = issue.get("prUrl")
        pr_branch = issue.get("prBranch")
        run_terminal = True
        # Refresh the PR from the current agent — a refix "new agent" may have opened
        # a newer PR we should now follow.
        if agent_id:
            pr = await cursor_client.pr_for_agent(CURSOR_API_KEY, agent_id)
            if pr:
                run_terminal = (pr.get("runStatus") in cursor_client.TERMINAL_RUN_STATUSES
                                or pr.get("agentStatus") in cursor_client.TERMINAL_RUN_STATUSES)
                if pr.get("prUrl") and pr["prUrl"] != pr_url:
                    pr_url = pr["prUrl"]
                    pr_branch = pr.get("branch")
                    patch = {"prUrl": pr_url, "prBranch": pr_branch}
                    parsed = github_client.parse_pr_url(pr_url)
                    if parsed:
                        patch["prNumber"] = parsed[2]
                    await self.service.update("issues", issue_id, patch)
        if not pr_url:
            return
        parsed = github_client.parse_pr_url(pr_url)
        if not parsed:
            return
        owner, repo, number = parsed
        pr_obj = await github_client.get_pr(owner, repo, number)
        if pr_obj.get("merged"):
            await self.service.transition("issues", issue_id, "closed")
            await self._set_notion(issue, N_MERGED)
            await self._notify(issue, f"Merged: *{issue.get('title')}*\n{pr_url}")
            log.info("issue %s prprep -> closed (merged)", issue_id)
            return
        sha = ((pr_obj.get("head") or {}).get("sha")) or ""
        runs = await github_client.check_runs(owner, repo, sha) if sha else []
        reviews = await github_client.pr_reviews(owner, repo, number)
        ci = github_client.ci_state(runs)
        cr = github_client.coderabbit_state(reviews, runs)
        await self.service.update("issues", issue_id, {"checkState": ci, "coderabbitState": cr})

        if ci == "passing" and cr == "clear":
            await self.service.transition("issues", issue_id, "pr_ready")
            await self._set_notion(issue, N_PR_READY)
            await self._notify(
                issue,
                f"PR green & CodeRabbit-clear — ready to merge: *{issue.get('title')}*\n{pr_url}")
            log.info("issue %s prprep -> pr_ready", issue_id)
            return

        if ci == "failing" or cr == "blocking":
            if not run_terminal:
                return  # the agent is still pushing — let it finish before re-fixing
            if sha and sha == issue.get("lastFixedSha"):
                return  # already dispatched a fix for this exact commit
            attempts = issue.get("fixAttempts") or 0
            if attempts >= MAX_FIX_ATTEMPTS:
                await self.service.transition("issues", issue_id, "needs_human")
                await self._set_notion(issue, N_NEEDS_HUMAN)
                await self._notify(
                    issue,
                    f"*{issue.get('title')}* still failing after {MAX_FIX_ATTEMPTS} fix attempts "
                    f"— needs a human.\n{pr_url}")
                log.info("issue %s prprep -> needs_human (cap %d hit)", issue_id, MAX_FIX_ATTEMPTS)
                return
            comments = await github_client.pr_review_comments(owner, repo, number)
            summary = github_client.failure_summary(runs, reviews, comments)
            new_agent_id = await self._dispatch_refix(
                issue, owner, repo, pr_url, pr_branch, summary, agent_id)
            await self.service.update("issues", issue_id, {
                "cursorAgentId": new_agent_id, "fixAttempts": attempts + 1, "lastFixedSha": sha,
            })
            await self._notify(
                issue,
                f"Fixing errors for *{issue.get('title')}* (attempt {attempts + 1}/"
                f"{MAX_FIX_ATTEMPTS})\nPR: {pr_url}")
            log.info("issue %s prprep refix #%d (ci=%s cr=%s)", issue_id, attempts + 1, ci, cr)
        # else: ci pending or CodeRabbit hasn't reviewed yet -> wait for the next tick

    async def _dispatch_refix(self, issue: dict[str, Any], owner: str, repo: str,
                              pr_url: str, pr_branch: Optional[str], summary: str,
                              agent_id: Optional[str]) -> str:
        """Dispatch a fix for a red PR and return the agent id now working it.

        In ``followup`` mode, posts a new run to the same agent (keeps its PR). In
        ``new_agent`` mode (default), launches a fresh agent from the PR branch so it
        builds on the prior fix.

        Args:
            issue: The issue row.
            owner: PR repo owner.
            repo: PR repo name.
            pr_url: The failing PR's URL.
            pr_branch: The PR's head branch (the new agent starts from it).
            summary: The assembled CI/CodeRabbit failure text.
            agent_id: The current agent id (reused in followup mode).

        Returns:
            The agent id working the fix after dispatch.
        """
        prompt = self._refix_prompt(issue, pr_url, pr_branch, summary)
        if REFIX_MODE == "followup" and agent_id:
            await cursor_client.add_followup(CURSOR_API_KEY, agent_id, prompt)
            return agent_id
        repo_url = github_client.repo_url_from_pr(owner, repo)
        new_id = f"bc-{uuid.uuid4()}"
        await cursor_client.launch_agent(
            CURSOR_API_KEY, prompt, repo_url, pr_branch or "main",
            agent_id=new_id, auto_create_pr=True, model=CURSOR_MODEL,
        )
        return new_id

    @staticmethod
    def _refix_prompt(issue: dict[str, Any], pr_url: str, pr_branch: Optional[str],
                      summary: str) -> str:
        """Build the instruction handed to the fix agent for a failing PR.

        Args:
            issue: The issue row (for the title/context).
            pr_url: The failing PR's URL.
            pr_branch: The PR's head branch.
            summary: The CI/CodeRabbit failure text.

        Returns:
            The full prompt string.
        """
        branch_note = (f" on branch `{pr_branch}`" if pr_branch else "")
        return "\n".join([
            f"You are fixing pull request {pr_url}{branch_note}, which addresses: "
            f"{issue.get('title')}.",
            "The PR is currently failing its checks. Fix EVERYTHING listed below, then commit and "
            f"push to the SAME pull request ({pr_url}) — do not open a new PR if one already exists.",
            "After pushing, make sure the CI checks pass and CodeRabbit's requested changes are "
            "resolved.",
            "",
            summary or "(no machine-readable failure detail was available; inspect the PR's failing "
            "checks and CodeRabbit review directly and fix them.)",
        ])

    async def _set_notion(self, issue: dict[str, Any], label: str) -> None:
        """Set the issue's Notion page status label (no-op without Notion/page).

        Args:
            issue: The issue row (provides ``notionPageId``).
            label: The Notion board status label to set.
        """
        if self.notion and issue.get("notionPageId"):
            try:
                await self.notion.set_status(issue["notionPageId"], label)
            except Exception:  # noqa: BLE001
                log.warning("cursor-tracker: notion set_status failed for %s", issue.get("_id"))

    async def _notify(self, issue: dict[str, Any], text: str) -> None:
        """Post a tracker update as a reply in the issue's fix thread.

        Threads under the dispatch message (``fixThreadTs``) so the whole fix saga
        stays in one Slack thread; falls back to a channel post when there's no
        thread root. No-op when Slack isn't configured.

        Args:
            issue: The issue row (provides ``fixThreadTs``).
            text: The message text.
        """
        if SLACK_FIX_CHANNEL and SLACK_BOT_TOKEN:
            await slack_client.post_message(
                SLACK_BOT_TOKEN, SLACK_FIX_CHANNEL, text,
                thread_ts=issue.get("fixThreadTs"),
            )
