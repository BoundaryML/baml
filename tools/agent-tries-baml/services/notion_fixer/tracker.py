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
  pr_ready  -- keep watching the ready-to-merge PR: a human merge -> closed; late human
               review comments -> dispatch a fix and pull it back to prprep

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
from bench_core import linear_client as lc
from bench_core.linear_client import LinearClient

log = logging.getLogger("cursor_tracker")

CURSOR_API_KEY = os.environ.get("ATB_CURSOR_API_KEY", "")
CURSOR_MODEL = os.environ.get("CURSOR_MODEL") or None
SLACK_BOT_TOKEN = os.environ.get("ATB_SLACK_BOT_TOKEN", "")
SLACK_FIX_CHANNEL = os.environ.get("ATB_SLACK_FIX_CHANNEL", "")
LINEAR_API_KEY = os.environ.get("ATB_LINEAR_TOKEN", "")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://new.boundaryml.com/atb")

TRACK_SECS = int(os.environ.get("CURSOR_TRACK_SECS", "60"))
# new_agent (launch a fresh agent from the PR branch each fix) | followup (POST a new
# run to the same agent, keeping its branch/PR).
REFIX_MODE = os.environ.get("CURSOR_REFIX_MODE", "new_agent")
MAX_FIX_ATTEMPTS = int(os.environ.get("CURSOR_MAX_FIX_ATTEMPTS", "3"))
TRACK_LIMIT = int(os.environ.get("CURSOR_TRACK_LIMIT", "100"))

# Linear status-group label ids for the tracker-driven statuses.
N_PRPREP = lc.LINEAR_STATUS_PR_PREP
N_PR_READY = lc.LINEAR_STATUS_READY_TO_MERGE
N_NEEDS_HUMAN = lc.LINEAR_STATUS_NEEDS_HUMAN
N_MERGED = lc.LINEAR_STATUS_MERGED


class CursorTracker:
    """Sweep that drives dispatched fix agents through tocursor -> prprep -> pr_ready."""

    def __init__(self, service):
        """Bind the service client and build a Linear client for board updates.

        Args:
            service: The ServiceClient used for all reads/writes.
        """
        self.service = service
        self.linear = LinearClient(LINEAR_API_KEY) if LINEAR_API_KEY else None

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
        """Track every issue currently in ``tocursor``, ``prprep``, or ``pr_ready`` once.

        A no-op when no Cursor API key is configured. Per-issue failures are logged
        and skipped so one bad PR can't stall the rest.
        """
        if not CURSOR_API_KEY:
            return
        # "fixing" is the legacy pre-tracker dispatched state; sweep it like tocursor
        # so any issue already in flight at deploy time isn't orphaned. "pr_ready" is
        # swept too so late human review on a ready-to-merge PR pulls it back to prprep.
        for status in ("tocursor", "fixing", "prprep", "pr_ready"):
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
            issue: The issue row (status is ``tocursor``, ``prprep``, or ``pr_ready``).
        """
        status = issue.get("status")
        if status in ("tocursor", "fixing"):
            await self._track_tocursor(issue)
        elif status == "pr_ready":
            await self._track_pr_ready(issue)
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
        run_terminal = pr.get("runStatus") in cursor_client.TERMINAL_RUN_STATUSES
        pr_url = pr.get("prUrl")
        # Cursor often fails to surface the PR (git.branches[].prUrl stays null even
        # after the agent opened one). When the run is over, fall back to GitHub and
        # look the PR up by the agent's branch, so a real PR + CodeRabbit review isn't
        # stranded as "no PR".
        if not pr_url and run_terminal:
            pr_url = await self._github_pr_url(pr.get("repoUrl"), pr.get("branch"))
        if pr_url:
            # Mark the card dirty so LinearPush re-renders the body with the PR link
            # in the Links section (set_status below only swaps the status label).
            patch = {"prUrl": pr_url, "prBranch": pr.get("branch"), "linearSyncStatus": "dirty"}
            parsed = github_client.parse_pr_url(pr_url)
            if parsed:
                patch["prNumber"] = parsed[2]
            await self.service.transition("issues", issue_id, "prprep", patch=patch)
            await self._set_linear(issue, N_PRPREP)
            await self._notify(issue, f"PR up for *{issue.get('title')}*\n{pr_url}")
            log.info("issue %s tocursor -> prprep (%s)", issue_id, pr_url)
            return
        # Run over and no PR on Cursor OR GitHub -> the agent genuinely produced none.
        if run_terminal:
            await self.service.transition("issues", issue_id, "needs_human")
            await self._set_linear(issue, N_NEEDS_HUMAN)
            await self._notify(
                issue,
                f"Cursor agent finished with no PR for *{issue.get('title')}* — needs a human.")
            log.info("issue %s tocursor -> needs_human (no PR)", issue_id)

    async def _github_pr_url(self, repo_url: Optional[str], branch: Optional[str]) -> Optional[str]:
        """Resolve the open PR URL for an agent's branch via GitHub (fallback).

        Args:
            repo_url: The repo URL Cursor reported for the agent's push.
            branch: The agent's head branch.

        Returns:
            The PR's html_url, or None when repo/branch is missing or no PR exists.
        """
        if not repo_url or not branch:
            return None
        parsed = github_client.parse_repo_url(repo_url)
        if not parsed:
            return None
        owner, repo = parsed
        try:
            pr = await github_client.open_pr_for_branch(owner, repo, branch)
        except Exception:  # noqa: BLE001
            log.warning("github PR lookup failed for %s/%s @ %s", owner, repo, branch)
            return None
        return (pr or {}).get("html_url")

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
            await self._set_linear(issue, N_MERGED)
            await self._notify(issue, f"Merged: *{issue.get('title')}*\n{pr_url}")
            log.info("issue %s prprep -> closed (merged)", issue_id)
            return
        sha = ((pr_obj.get("head") or {}).get("sha")) or ""
        runs = await github_client.check_runs(owner, repo, sha) if sha else []
        reviews = await github_client.pr_reviews(owner, repo, number)
        ci = github_client.ci_state(runs)
        cr = github_client.coderabbit_state(reviews, runs)
        await self.service.update("issues", issue_id, {"checkState": ci, "coderabbitState": cr})

        if ci == "passing" and cr == "clear" and run_terminal:
            # run_terminal gate: don't declare "ready to merge" while a just-dispatched
            # fix agent is still working the PR (e.g. addressing human review comments) —
            # the PR can be green on the old head before the agent has pushed.
            await self.service.transition("issues", issue_id, "pr_ready")
            await self._set_linear(issue, N_PR_READY)
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
                await self._set_linear(issue, N_NEEDS_HUMAN)
                await self._notify(
                    issue,
                    f"*{issue.get('title')}* still failing after {MAX_FIX_ATTEMPTS} fix attempts "
                    f"— needs a human.\n{pr_url}")
                log.info("issue %s prprep -> needs_human (cap %d hit)", issue_id, MAX_FIX_ATTEMPTS)
                return
            comments = await github_client.pr_review_comments(owner, repo, number)
            summary = github_client.failure_summary(runs, reviews, comments)
            # Collect the CodeRabbit comments we're addressing to preview them in the
            # Slack ack, so the thread shows exactly what the agent noticed and is fixing.
            cr_items: list[dict[str, Any]] = []
            if cr == "blocking":
                cr_login = github_client.CODERABBIT_LOGIN
                cr_items = [rv for rv in reviews
                            if (rv.get("user") or {}).get("login") == cr_login
                            and rv.get("state") == "CHANGES_REQUESTED"]
                cr_items += [c for c in comments
                             if (c.get("user") or {}).get("login") == cr_login]
            new_agent_id = await self._dispatch_refix(
                issue, owner, repo, pr_url, pr_branch, summary, agent_id)
            await self.service.update("issues", issue_id, {
                "cursorAgentId": new_agent_id, "fixAttempts": attempts + 1, "lastFixedSha": sha,
            })
            reasons = []
            if cr == "blocking":
                reasons.append("CodeRabbit's requested changes")
            if ci == "failing":
                reasons.append("failing CI checks")
            await self._notify(issue, self._responding_msg(
                issue.get("title"), attempts + 1, pr_url,
                " and ".join(reasons) or "review feedback", self._previews(cr_items)))
            log.info("issue %s prprep refix #%d (ci=%s cr=%s)", issue_id, attempts + 1, ci, cr)
        # else: ci pending or CodeRabbit hasn't reviewed yet -> wait for the next tick

    async def _track_pr_ready(self, issue: dict[str, Any]) -> None:
        """Re-check a ready-to-merge PR for late human review and pull it back to prep.

        A PR can reach ``pr_ready`` and then get human review comments before anyone
        merges it. On genuinely new human feedback (newer than the persisted
        high-water mark) it dispatches a fix and moves the issue back to ``prprep``.
        A human merge in the meantime closes it.

        Args:
            issue: The ``pr_ready`` issue row.
        """
        issue_id = issue["_id"]
        pr_url = issue.get("prUrl")
        pr_branch = issue.get("prBranch")
        if not pr_url:
            return
        parsed = github_client.parse_pr_url(pr_url)
        if not parsed:
            return
        owner, repo, number = parsed
        pr_obj = await github_client.get_pr(owner, repo, number)
        if pr_obj.get("merged"):
            await self.service.transition("issues", issue_id, "closed")
            await self._set_linear(issue, N_MERGED)
            await self._notify(issue, f"Merged: *{issue.get('title')}*\n{pr_url}")
            log.info("issue %s pr_ready -> closed (merged)", issue_id)
            return
        # Detect human review newer than the last comment we acted on (a persisted
        # high-water mark). This does NOT depend on the 👀 reaction succeeding, so
        # pickup is robust even when the reaction POST is forbidden for our token.
        review_comments = await github_client.pr_review_comments(owner, repo, number)
        convo_comments = await github_client.issue_comments(owner, repo, number)
        hwm = issue.get("lastHumanCommentAt")
        tagged = self._new_human_comments(review_comments, convo_comments, hwm)
        if not tagged:
            return
        new_review = [c for k, c in tagged if k == "review"]
        new_convo = [c for k, c in tagged if k == "issue"]
        new_human = new_review + new_convo
        newest = max((c.get("created_at") or "") for c in new_human)
        attempts = issue.get("fixAttempts") or 0
        if attempts >= MAX_FIX_ATTEMPTS:
            # Advance the high-water mark so we don't re-flag the same comments forever.
            await self.service.transition("issues", issue_id, "needs_human",
                                          patch={"lastHumanCommentAt": newest})
            await self._set_linear(issue, N_NEEDS_HUMAN)
            await self._notify(
                issue,
                f"New review on *{issue.get('title')}* but already at {MAX_FIX_ATTEMPTS} fix "
                f"attempts — needs a human.\n{pr_url}")
            log.info("issue %s pr_ready -> needs_human (cap %d hit)", issue_id, MAX_FIX_ATTEMPTS)
            return
        summary = github_client.human_comment_summary(new_review, new_convo)
        sha = ((pr_obj.get("head") or {}).get("sha")) or ""
        agent_id = self._agent_id(issue)
        new_agent_id = await self._dispatch_refix(
            issue, owner, repo, pr_url, pr_branch, summary, agent_id)
        await self.service.transition("issues", issue_id, "prprep", patch={
            "cursorAgentId": new_agent_id, "fixAttempts": attempts + 1, "lastFixedSha": sha,
            "lastHumanCommentAt": newest,
        })
        await self._set_linear(issue, N_PRPREP)
        await self._notify(issue, self._responding_msg(
            issue.get("title"), attempts + 1, pr_url, "reviewer comments",
            self._previews(new_human)))
        log.info("issue %s pr_ready -> prprep (human review, refix #%d)", issue_id, attempts + 1)

    @staticmethod
    def _new_human_comments(
        review_comments: list[dict[str, Any]], convo_comments: list[dict[str, Any]],
        hwm: Optional[str],
    ) -> list[tuple[str, dict[str, Any]]]:
        """Return ``(kind, comment)`` for human comments newer than the high-water mark.

        A comment counts when its author is a real user — bots like CodeRabbit/Cursor
        are skipped (handled by the CodeRabbit/CI gates) — and its ``created_at`` is
        strictly greater than ``hwm`` (ISO-8601 UTC, so a lexical compare is
        chronological). ``kind`` is ``"review"`` for inline diff comments or
        ``"issue"`` for conversation comments (selects the reaction endpoint).

        Args:
            review_comments: The PR's inline review comments.
            convo_comments: The PR's conversation comments.
            hwm: The last-acted-on comment timestamp, or None to take all of them.

        Returns:
            The new human comments tagged with their comment ``kind``.
        """
        out: list[tuple[str, dict[str, Any]]] = []
        for kind, comments in (("review", review_comments), ("issue", convo_comments)):
            for c in comments:
                if (c.get("user") or {}).get("type") != "User":
                    continue  # skip bots (CodeRabbit, Cursor) — handled elsewhere
                created = c.get("created_at") or ""
                if hwm and created <= hwm:
                    continue
                out.append((kind, c))
        return out

    @staticmethod
    def _previews(items: list[dict[str, Any]], limit: int = 3, maxlen: int = 160) -> list[str]:
        """Format ``@author: body`` previews of comments/reviews for a Slack ack.

        Args:
            items: Comment/review dicts (each with ``user.login`` and ``body``).
            limit: Max number of previews to return.
            maxlen: Max characters per preview body (truncated with an ellipsis).

        Returns:
            Up to ``limit`` single-line ``@login: text`` strings (empty bodies skipped).
        """
        out: list[str] = []
        for it in items:
            body = " ".join((it.get("body") or "").split())
            if not body:
                continue
            who = (it.get("user") or {}).get("login") or "reviewer"
            if len(body) > maxlen:
                body = body[: maxlen - 1].rstrip() + "…"
            out.append(f"@{who}: {body}")
            if len(out) >= limit:
                break
        return out

    @staticmethod
    def _responding_msg(title: Optional[str], attempt: int, pr_url: str, reason: str,
                        previews: list[str]) -> str:
        """Build the "noticed a comment, responding now" Slack thread message.

        Args:
            title: The issue title.
            attempt: The fix attempt number being dispatched.
            pr_url: The PR URL.
            reason: What the agent is responding to (e.g. "CodeRabbit's requested
                changes", "reviewer comments").
            previews: Short ``@author: body`` quotes of the comments being addressed.

        Returns:
            The formatted Slack message text.
        """
        msg = f"Responding to {reason} on *{title}* (attempt {attempt}/{MAX_FIX_ATTEMPTS})."
        if previews:
            msg += "\n" + "\n".join(f"> {p}" for p in previews)
        msg += f"\nPR: {pr_url}"
        return msg

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
            "The PR has unresolved feedback (failing checks, CodeRabbit, and/or human reviewer "
            "comments). Address EVERYTHING listed below, then commit and "
            f"push to the SAME pull request ({pr_url}) — do not open a new PR if one already exists.",
            "After pushing, make sure the CI checks pass and all requested changes (CodeRabbit and "
            "human reviewers) are resolved.",
            "If a new PR does get created, give it a precise descriptive title (NEVER the "
            'placeholder "Pull request template") — e.g. `gh pr edit --title "<precise title>"`.',
            "Document every function, method, and type you add or change with a docstring "
            '(Rust `///`, TypeScript JSDoc `/** */`, Python `"""..."""`); always include docstrings.',
            "",
            summary or "(no machine-readable failure detail was available; inspect the PR's failing "
            "checks and CodeRabbit review directly and fix them.)",
        ])

    async def _set_linear(self, issue: dict[str, Any], label_id: str) -> None:
        """Swap the issue's Linear status label (no-op without Linear/issue id).

        Args:
            issue: The issue row (provides ``linearIssueId``).
            label_id: The Linear status-group label id to set.
        """
        if self.linear and issue.get("linearIssueId"):
            try:
                await self.linear.set_status(issue["linearIssueId"], label_id)
            except Exception:  # noqa: BLE001
                log.warning("cursor-tracker: linear set_status failed for %s", issue.get("_id"))

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
