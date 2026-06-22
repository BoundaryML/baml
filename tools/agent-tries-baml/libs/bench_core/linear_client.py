"""Minimal Linear GraphQL client — the board layer for the pipeline.

Creates/updates issues with a title, a Markdown description, and a single
status-group label (Linear's ``agent-tries-baml-status`` group allows one label
at a time), and reads an issue's status label + comments back.

Linear labels REPLACE rather than merge on update, and a label group allows only
one label, so "set the status" is a read-modify-write: read the issue's current
label ids, drop any that belong to the status group, add the new one, and write
the full set via ``issueUpdate(labelIds)``. Descriptions and comments are native
Markdown, so the structured body is built as Markdown and passed straight through.
"""

from __future__ import annotations

import logging
import os
import re
from typing import Any, Iterable, Optional

import httpx

log = logging.getLogger("linear_client")

LINEAR_API = os.environ.get("LINEAR_API_BASE", "https://api.linear.app/graphql")
# BoundaryML (internal) team. Every issue is created against this single team;
# there is no skill/language split on the board (kind is intentionally dropped).
LINEAR_TEAM_ID = os.environ.get("LINEAR_TEAM_ID", "179250f3-902f-491e-94f8-8256292e8386")

def _required_id(name: str) -> str:
    """Read a required Linear id (status-group label or workflow state) from env.

    These ids are deployment config, injected as secrets (Fly secrets / Infisical)
    rather than hardcoded, so a different workspace points at its own ids and no
    real ids are baked into source.

    Args:
        name: The environment variable holding the Linear id.

    Returns:
        The id string.

    Raises:
        RuntimeError: When the variable is unset or empty — a misconfigured deploy
            should fail loudly at startup, not silently write to the wrong board.
    """
    v = os.environ.get(name)
    if not v:
        raise RuntimeError(
            f"{name} is not set; the Linear status ids are configured as secrets "
            "(see docs/configuration.md), not hardcoded")
    return v


# Status-group label ids (group parent = agent-tries-baml-status). Linear enforces
# one label per group, so exactly one of these is ever set on an issue. Sourced from
# the environment (deployed as secrets), never hardcoded.
LINEAR_STATUS_NOT_STARTED = _required_id("LINEAR_STATUS_NOT_STARTED")
LINEAR_STATUS_APPROVED = _required_id("LINEAR_STATUS_APPROVED")
LINEAR_STATUS_TO_CURSOR = _required_id("LINEAR_STATUS_TO_CURSOR")
LINEAR_STATUS_PR_PREP = _required_id("LINEAR_STATUS_PR_PREP")
LINEAR_STATUS_READY_TO_MERGE = _required_id("LINEAR_STATUS_READY_TO_MERGE")
LINEAR_STATUS_NEEDS_HUMAN = _required_id("LINEAR_STATUS_NEEDS_HUMAN")
LINEAR_STATUS_REDRAFT = _required_id("LINEAR_STATUS_REDRAFT")
LINEAR_STATUS_MERGED = _required_id("LINEAR_STATUS_MERGED")
# Human-only terminal status: moving a card here deletes the issue from the
# pipeline DB entirely (handled by ingress /linear/webhook). The bot never sets it.
LINEAR_STATUS_REJECTED = _required_id("LINEAR_STATUS_REJECTED")

# The full set of status-group label ids, used to STRIP the existing status label
# before adding a new one (so the group never carries two at once).
STATUS_GROUP_LABEL_IDS = frozenset({
    LINEAR_STATUS_NOT_STARTED, LINEAR_STATUS_APPROVED, LINEAR_STATUS_TO_CURSOR,
    LINEAR_STATUS_PR_PREP, LINEAR_STATUS_READY_TO_MERGE, LINEAR_STATUS_NEEDS_HUMAN,
    LINEAR_STATUS_REDRAFT, LINEAR_STATUS_MERGED, LINEAR_STATUS_REJECTED,
})

# Canonical human names for each status-group label id (the child label names in
# Linear). Lets get_status / status_label_name report a name without an id->name
# lookup against the API on every read.
STATUS_LABEL_NAMES: dict[str, str] = {
    LINEAR_STATUS_NOT_STARTED: "not-started",
    LINEAR_STATUS_APPROVED: "approved",
    LINEAR_STATUS_TO_CURSOR: "to-cursor",
    LINEAR_STATUS_PR_PREP: "pr-prep",
    LINEAR_STATUS_READY_TO_MERGE: "ready-to-merge",
    LINEAR_STATUS_NEEDS_HUMAN: "needs-human",
    LINEAR_STATUS_REDRAFT: "redraft",
    LINEAR_STATUS_MERGED: "merged",
    LINEAR_STATUS_REJECTED: "rejected",
}

# Linear native workflow-status (state) ids — the board's Status column. ATB keeps
# its fine-grained pipeline stage in the status-group LABEL, and mirrors it onto one
# of these coarse Linear statuses so the native board view is meaningful too. Also
# sourced from the environment (deployed as secrets), never hardcoded.
LINEAR_STATE_UNCOMMITTED = _required_id("LINEAR_STATE_UNCOMMITTED")
LINEAR_STATE_IN_PROGRESS = _required_id("LINEAR_STATE_IN_PROGRESS")
LINEAR_STATE_IN_REVIEW = _required_id("LINEAR_STATE_IN_REVIEW")
LINEAR_STATE_DONE = _required_id("LINEAR_STATE_DONE")
LINEAR_STATE_CANCELED = _required_id("LINEAR_STATE_CANCELED")

# ATB status-group label id -> Linear workflow status id. Anything not listed here
# (e.g. pr-prep, ready-to-merge) falls through to In Review via state_id_for_label.
STATUS_LABEL_TO_STATE: dict[str, str] = {
    LINEAR_STATUS_NOT_STARTED: LINEAR_STATE_UNCOMMITTED,
    LINEAR_STATUS_APPROVED: LINEAR_STATE_IN_PROGRESS,
    LINEAR_STATUS_TO_CURSOR: LINEAR_STATE_IN_PROGRESS,
    LINEAR_STATUS_REDRAFT: LINEAR_STATE_IN_REVIEW,
    LINEAR_STATUS_NEEDS_HUMAN: LINEAR_STATE_CANCELED,
    LINEAR_STATUS_MERGED: LINEAR_STATE_DONE,
    LINEAR_STATUS_REJECTED: LINEAR_STATE_CANCELED,
}


def state_id_for_label(status_label_id: str) -> str:
    """Map an ATB status-group label id to its Linear workflow status id.

    Args:
        status_label_id: The ATB status-group label id being set.

    Returns:
        The Linear workflow status id to set alongside the label; unmapped labels
        (pr-prep, ready-to-merge) fall through to In Review.
    """
    return STATUS_LABEL_TO_STATE.get(status_label_id, LINEAR_STATE_IN_REVIEW)


class LinearError(RuntimeError):
    """Raised when the Linear GraphQL API returns an ``errors`` array."""


def status_label_name(label_ids: Iterable[str]) -> Optional[str]:
    """Return the status-group label's human name from a set of label ids.

    Args:
        label_ids: All label ids currently on an issue.

    Returns:
        The canonical name (``"approved"``, ``"redraft"``, …) of the first id that
        belongs to the status group, or None when none do.
    """
    for lid in label_ids:
        if lid in STATUS_GROUP_LABEL_IDS:
            return STATUS_LABEL_NAMES.get(lid)
    return None


def issue_body_md(
    body: str, evidence_links: list[str], suggestion: Optional[str] = None,
    category: Optional[str] = None, repro: Optional[str] = None,
    issue_link: Optional[str] = None, pr_url: Optional[str] = None,
) -> str:
    """Build an issue's Markdown description (the Linear card body).

    Renders an optional Links line at the top (dashboard issue + PR), an optional
    Category line, the description, a verified Reproduction code block, a Suggested
    fix section, and an Evidence bullet list. Linear descriptions are native
    Markdown, so the result is passed straight through.

    Args:
        body: Markdown description.
        evidence_links: Links rendered as an Evidence bullet list.
        suggestion: Optional suggested-fix Markdown.
        category: Optional category label rendered as a bold line.
        repro: Optional verified repro, rendered verbatim in a code block.
        issue_link: Optional dashboard URL for the issue's own page.
        pr_url: Optional pull-request URL once a fix PR exists.

    Returns:
        The assembled Markdown body.
    """
    parts: list[str] = []
    link_parts = []
    if issue_link:
        link_parts.append(f"[Issue]({issue_link})")
    if pr_url:
        link_parts.append(f"[PR]({pr_url})")
    if link_parts:
        parts.append("**Links:** " + " · ".join(link_parts))
    if category:
        parts.append(f"**Category:** {category}")
    if body:
        parts.append(body)
    if repro:
        # BAML reads close to TypeScript, so highlight the repro as such.
        parts.append("## Reproduction\n\n```typescript\n" + repro + "\n```")
    if suggestion:
        parts.append("## Suggested fix\n\n" + suggestion)
    if evidence_links:
        items = []
        for i, link in enumerate(evidence_links, 1):
            call = re.search(r"call=(\d+)", link)
            label = f"run {i}" + (f" · call {call.group(1)}" if call else "")
            items.append(f"- [{label}]({link})")
        parts.append("## Evidence\n\n" + "\n".join(items))
    return "\n\n".join(parts)


class LinearClient:
    """Minimal Linear GraphQL client for creating and updating issue cards."""

    def __init__(self, api_key: str, *, team_id: str = LINEAR_TEAM_ID):
        """Build the auth headers for Linear GraphQL requests.

        Args:
            api_key: Linear API key. A personal API key is sent verbatim as the
                Authorization header (Linear's convention); OAuth tokens would be
                ``Bearer <token>``.
            team_id: The Linear team issues are created under.
        """
        self._headers = {"Authorization": api_key, "Content-Type": "application/json"}
        self._team_id = team_id

    async def _gql(self, query: str, variables: dict[str, Any]) -> dict[str, Any]:
        """Execute one GraphQL operation and return its ``data`` payload.

        Args:
            query: The GraphQL query/mutation text.
            variables: The operation's variables.

        Returns:
            The ``data`` object from the response.

        Raises:
            httpx.HTTPStatusError: On a non-2xx HTTP response.
            LinearError: When the response carries a GraphQL ``errors`` array.
        """
        async with httpx.AsyncClient(timeout=30.0) as c:
            r = await c.post(LINEAR_API, json={"query": query, "variables": variables},
                             headers=self._headers)
            r.raise_for_status()
            payload = r.json()
        if payload.get("errors"):
            raise LinearError(str(payload["errors"]))
        return payload.get("data") or {}

    async def _current_label_ids(self, issue_id: str) -> list[str]:
        """Read all label ids currently on an issue.

        Args:
            issue_id: The Linear issue id.

        Returns:
            The list of label ids (empty when the issue has none).
        """
        data = await self._gql(
            "query($id: String!) { issue(id: $id) { labels { nodes { id } } } }",
            {"id": issue_id},
        )
        nodes = (((data.get("issue") or {}).get("labels") or {}).get("nodes")) or []
        return [n["id"] for n in nodes if n.get("id")]

    @staticmethod
    def _swap_status_label(current: Iterable[str], status_label_id: str) -> list[str]:
        """Return the label-id set with the status-group label swapped out.

        Drops any existing status-group label (Linear allows only one per group)
        and adds ``status_label_id``, preserving every non-status label.

        Args:
            current: The issue's current label ids.
            status_label_id: The status-group label id to set.

        Returns:
            The full label-id list to write back via issueUpdate.
        """
        kept = [lid for lid in current if lid not in STATUS_GROUP_LABEL_IDS]
        return kept + [status_label_id]

    async def create_issue(
        self, title: str, status_label_id: str, body: str,
        evidence_links: list[str], suggestion: Optional[str] = None,
        category: Optional[str] = None, repro: Optional[str] = None,
        issue_link: Optional[str] = None, pr_url: Optional[str] = None,
    ) -> str:
        """Create a Linear issue from Markdown with its initial status label.

        Args:
            title: Issue title.
            status_label_id: The status-group label id to set on creation.
            body: Markdown description.
            evidence_links: Links rendered as an Evidence bullet list.
            suggestion: Optional suggested-fix Markdown.
            category: Optional category label.
            repro: Optional verified repro rendered as a code block.
            issue_link: Optional dashboard URL for the issue's own page.
            pr_url: Optional pull-request URL once a fix PR exists.

        Returns:
            The id of the newly created issue.

        Raises:
            LinearError: When the mutation fails or reports success=false.
        """
        description = issue_body_md(
            body, evidence_links, suggestion, category, repro, issue_link, pr_url)
        data = await self._gql(
            """
            mutation($input: IssueCreateInput!) {
              issueCreate(input: $input) { success issue { id } }
            }
            """,
            {"input": {
                "teamId": self._team_id, "title": title,
                "description": description, "labelIds": [status_label_id],
                "stateId": state_id_for_label(status_label_id),
            }},
        )
        result = data.get("issueCreate") or {}
        issue = result.get("issue") or {}
        if not result.get("success") or not issue.get("id"):
            raise LinearError(f"issueCreate failed: {data}")
        return issue["id"]

    async def update_issue(
        self, issue_id: str, title: str, status_label_id: str, body: str,
        evidence_links: list[str], suggestion: Optional[str] = None,
        category: Optional[str] = None, repro: Optional[str] = None,
        issue_link: Optional[str] = None, pr_url: Optional[str] = None,
    ) -> None:
        """Re-render an issue in place: title, status label, and Markdown body.

        Reads the current labels, swaps the status-group label for
        ``status_label_id`` (preserving any other labels), and writes the title +
        description + full label set in one ``issueUpdate``.

        Args:
            issue_id: The Linear issue id.
            title: New issue title.
            status_label_id: The status-group label id to set.
            body: Markdown description.
            evidence_links: Links rendered as an Evidence bullet list.
            suggestion: Optional suggested-fix Markdown.
            category: Optional category label.
            repro: Optional verified repro rendered as a code block.
            issue_link: Optional dashboard URL for the issue's own page.
            pr_url: Optional pull-request URL once a fix PR exists.

        Raises:
            LinearError: When the mutation fails.
        """
        description = issue_body_md(
            body, evidence_links, suggestion, category, repro, issue_link, pr_url)
        current = await self._current_label_ids(issue_id)
        label_ids = self._swap_status_label(current, status_label_id)
        await self._issue_update(issue_id, {
            "title": title, "description": description, "labelIds": label_ids,
            "stateId": state_id_for_label(status_label_id),
        })

    async def set_status(self, issue_id: str, status_label_id: str) -> None:
        """Swap an issue's status-group label (no body rewrite).

        Used by FixDispatch / the cursor-tracker / bug-verify for status-only
        moves. A read-modify-write: read current labels, drop the status-group
        label, add ``status_label_id``, write the set back.

        Args:
            issue_id: The Linear issue id.
            status_label_id: The status-group label id to set.

        Raises:
            LinearError: When the mutation fails.
        """
        current = await self._current_label_ids(issue_id)
        label_ids = self._swap_status_label(current, status_label_id)
        await self._issue_update(issue_id, {
            "labelIds": label_ids, "stateId": state_id_for_label(status_label_id),
        })

    async def _issue_update(self, issue_id: str, input_fields: dict[str, Any]) -> None:
        """Run an ``issueUpdate`` mutation with the given input fields.

        Args:
            issue_id: The Linear issue id.
            input_fields: The IssueUpdateInput fields to set.

        Raises:
            LinearError: When the mutation reports success=false.
        """
        data = await self._gql(
            """
            mutation($id: String!, $input: IssueUpdateInput!) {
              issueUpdate(id: $id, input: $input) { success }
            }
            """,
            {"id": issue_id, "input": input_fields},
        )
        if not (data.get("issueUpdate") or {}).get("success"):
            raise LinearError(f"issueUpdate failed: {data}")

    async def get_status(self, issue_id: str) -> Optional[str]:
        """Read an issue's current status-group label name.

        Args:
            issue_id: The Linear issue id.

        Returns:
            The status label name (``"approved"``, ``"redraft"``, …), or None
            when no status-group label is set.
        """
        return status_label_name(await self._current_label_ids(issue_id))

    async def add_comment(self, issue_id: str, body_md: str) -> None:
        """Add a Markdown comment to an issue.

        Args:
            issue_id: The Linear issue id.
            body_md: The comment body (native Markdown).

        Raises:
            LinearError: When the mutation fails.
        """
        data = await self._gql(
            """
            mutation($input: CommentCreateInput!) {
              commentCreate(input: $input) { success }
            }
            """,
            {"input": {"issueId": issue_id, "body": body_md}},
        )
        if not (data.get("commentCreate") or {}).get("success"):
            raise LinearError(f"commentCreate failed: {data}")

    async def get_comments(self, issue_id: str) -> list[dict[str, Any]]:
        """Fetch all comments on an issue, following pagination.

        Args:
            issue_id: The Linear issue id.

        Returns:
            A list of ``{"text", "created_time", "author"}`` dicts in Linear
            order (the shape the redraft feedback renderer expects).

        Raises:
            LinearError: When a query fails.
        """
        out: list[dict[str, Any]] = []
        cursor: Optional[str] = None
        while True:
            data = await self._gql(
                """
                query($id: String!, $after: String) {
                  issue(id: $id) {
                    comments(first: 100, after: $after) {
                      nodes { body createdAt user { id } }
                      pageInfo { hasNextPage endCursor }
                    }
                  }
                }
                """,
                {"id": issue_id, "after": cursor},
            )
            comments = (((data.get("issue") or {}).get("comments")) or {})
            for cm in comments.get("nodes") or []:
                out.append({
                    "text": cm.get("body") or "",
                    "created_time": cm.get("createdAt"),
                    "author": (cm.get("user") or {}).get("id"),
                })
            info = comments.get("pageInfo") or {}
            if not info.get("hasNextPage"):
                break
            cursor = info.get("endCursor")
        return out

    async def find_issue_by_title(self, title: str) -> Optional[str]:
        """Find a status-bearing team issue with an exact-matching title.

        Used to adopt a card that was already imported into Linear instead of
        creating a duplicate. Only issues that carry a status-group label are
        considered (so unrelated team issues never match). On more than one match
        the lookup is ambiguous, so it logs and returns None (skip, don't risk
        binding to the wrong card) rather than guessing.

        Args:
            title: The exact issue title to match.

        Returns:
            The single matching issue id, or None when there is no unambiguous
            match.

        Raises:
            LinearError: When the query fails.
        """
        data = await self._gql(
            """
            query($filter: IssueFilter!) {
              issues(filter: $filter, first: 50) {
                nodes { id labels { nodes { id } } }
              }
            }
            """,
            {"filter": {"team": {"id": {"eq": self._team_id}},
                        "title": {"eq": title}}},
        )
        nodes = (((data.get("issues") or {}).get("nodes")) or [])
        matches = [
            n["id"] for n in nodes
            if any(lbl.get("id") in STATUS_GROUP_LABEL_IDS
                   for lbl in ((n.get("labels") or {}).get("nodes") or []))
        ]
        if len(matches) == 1:
            return matches[0]
        if len(matches) > 1:
            log.warning("linear: %d status-bearing issues match title %r; skipping adopt",
                        len(matches), title)
        return None
