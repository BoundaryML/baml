"""Minimal GitHub REST client for tracking a fix PR's CI + CodeRabbit state.

The cursor-tracker uses this to read a pull request's check-runs and reviews and
decide whether the PR is passing, failing, blocked by CodeRabbit, or merged — and
to assemble the failure text handed to a follow-up fix agent. Auth reuses the
``ATB_GITHUB_TOKEN`` already provisioned for the build/changelog readers
(see services/api/routers/baml_builds.py:_gh_headers).
"""

from __future__ import annotations

import os
import re
from typing import Any, Optional

import httpx

GITHUB_API_BASE = os.environ.get("GITHUB_API_BASE", "https://api.github.com")
# The GitHub login CodeRabbit posts reviews/comments as.
CODERABBIT_LOGIN = os.environ.get("CODERABBIT_LOGIN", "coderabbitai[bot]")

# Completed check-run conclusions that mean the check failed (PR is red).
_FAIL_CONCLUSIONS = frozenset(
    {"failure", "timed_out", "cancelled", "action_required", "startup_failure"}
)

_PR_URL_RE = re.compile(r"github\.com/([^/]+)/([^/]+)/pull/(\d+)")


def _headers() -> dict[str, str]:
    """Build GitHub API headers, bearer-authed when ``ATB_GITHUB_TOKEN`` is set.

    Returns:
        A headers dict for the GitHub REST API.
    """
    h = {"Accept": "application/vnd.github+json"}
    token = os.environ.get("ATB_GITHUB_TOKEN")
    if token:
        h["Authorization"] = f"Bearer {token}"
    return h


def parse_pr_url(url: str) -> Optional[tuple[str, str, int]]:
    """Parse a GitHub PR URL into (owner, repo, number).

    Args:
        url: A pull-request URL like ``https://github.com/owner/repo/pull/123``.

    Returns:
        ``(owner, repo, number)``, or None when the URL isn't a PR URL.
    """
    m = _PR_URL_RE.search(url or "")
    if not m:
        return None
    return m.group(1), m.group(2), int(m.group(3))


def repo_url_from_pr(owner: str, repo: str) -> str:
    """Return the ``https://github.com/<owner>/<repo>`` URL for a refix agent."""
    return f"https://github.com/{owner}/{repo}"


async def get_pr(owner: str, repo: str, number: int, *, timeout: float = 30.0) -> dict[str, Any]:
    """Fetch a pull request (head sha, merged flag, mergeable state).

    Args:
        owner: Repo owner.
        repo: Repo name.
        number: PR number.
        timeout: HTTP timeout in seconds.

    Returns:
        The PR object.

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(f"{GITHUB_API_BASE}/repos/{owner}/{repo}/pulls/{number}",
                        headers=_headers())
        r.raise_for_status()
        return r.json()


async def check_runs(owner: str, repo: str, sha: str, *, timeout: float = 30.0) -> list[dict[str, Any]]:
    """List the check-runs for a commit (the PR head sha).

    Args:
        owner: Repo owner.
        repo: Repo name.
        sha: Commit sha to read checks for.
        timeout: HTTP timeout in seconds.

    Returns:
        The ``check_runs`` array (each with ``name``/``status``/``conclusion``/``output``).

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(f"{GITHUB_API_BASE}/repos/{owner}/{repo}/commits/{sha}/check-runs",
                        headers=_headers(), params={"per_page": 100})
        r.raise_for_status()
        return r.json().get("check_runs", []) or []


async def pr_reviews(owner: str, repo: str, number: int, *, timeout: float = 30.0) -> list[dict[str, Any]]:
    """List a PR's reviews (used to find CodeRabbit's CHANGES_REQUESTED).

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(f"{GITHUB_API_BASE}/repos/{owner}/{repo}/pulls/{number}/reviews",
                        headers=_headers(), params={"per_page": 100})
        r.raise_for_status()
        return r.json() or []


async def pr_review_comments(owner: str, repo: str, number: int, *,
                             timeout: float = 30.0) -> list[dict[str, Any]]:
    """List a PR's inline review comments (CodeRabbit's per-line feedback).

    Raises:
        httpx.HTTPStatusError: On a non-2xx response.
    """
    async with httpx.AsyncClient(timeout=timeout) as c:
        r = await c.get(f"{GITHUB_API_BASE}/repos/{owner}/{repo}/pulls/{number}/comments",
                        headers=_headers(), params={"per_page": 100})
        r.raise_for_status()
        return r.json() or []


# ---------- pure decision helpers (no network; unit-tested) ----------

def ci_state(runs: list[dict[str, Any]]) -> str:
    """Reduce a PR's check-runs to one of ``passing`` | ``failing`` | ``pending``.

    ``pending`` wins over ``failing`` (wait for all checks to complete before
    declaring the PR red, so a fix addresses every failure at once). Empty checks
    are treated as ``passing`` — the CodeRabbit gate (see :func:`coderabbit_state`)
    still holds the PR until a review lands, so this can't prematurely green a PR on
    a repo that reviews every PR.

    Args:
        runs: The check-run objects for the head sha.

    Returns:
        ``"passing"``, ``"failing"``, or ``"pending"``.
    """
    any_pending = False
    any_fail = False
    for r in runs:
        if r.get("status") != "completed":
            any_pending = True
        elif r.get("conclusion") in _FAIL_CONCLUSIONS:
            any_fail = True
    if any_pending:
        return "pending"
    if any_fail:
        return "failing"
    return "passing"


def coderabbit_state(reviews: list[dict[str, Any]], runs: list[dict[str, Any]],
                     login: str = CODERABBIT_LOGIN) -> str:
    """Reduce CodeRabbit's signals to ``blocking`` | ``clear`` | ``none``.

    Blocking when CodeRabbit's latest review is ``CHANGES_REQUESTED`` (its Request
    Changes Workflow) or a check-run named like "coderabbit" failed. Clear when
    CodeRabbit has reviewed and isn't blocking. None when CodeRabbit hasn't weighed
    in yet (so the tracker keeps waiting rather than greening the PR early).

    Args:
        reviews: The PR's reviews.
        runs: The PR's check-runs.
        login: CodeRabbit's GitHub login.

    Returns:
        ``"blocking"``, ``"clear"``, or ``"none"``.
    """
    cr_reviews = [rv for rv in reviews if (rv.get("user") or {}).get("login") == login]
    # latest CodeRabbit review by submission order
    latest = cr_reviews[-1] if cr_reviews else None
    if latest and latest.get("state") == "CHANGES_REQUESTED":
        return "blocking"
    for r in runs:
        name = (r.get("name") or "").lower()
        if "coderabbit" in name and r.get("status") == "completed" \
                and r.get("conclusion") in _FAIL_CONCLUSIONS:
            return "blocking"
    if cr_reviews:
        return "clear"
    # a passing coderabbit check also counts as a (clearing) signal
    for r in runs:
        if "coderabbit" in (r.get("name") or "").lower() and r.get("status") == "completed":
            return "clear"
    return "none"


def failure_summary(runs: list[dict[str, Any]], reviews: list[dict[str, Any]],
                    comments: list[dict[str, Any]], login: str = CODERABBIT_LOGIN) -> str:
    """Assemble the failure text handed to a follow-up fix agent.

    Lists each failing CI check (name + conclusion + output summary + link) and
    CodeRabbit's requested changes (review bodies + inline comments).

    Args:
        runs: The PR's check-runs.
        reviews: The PR's reviews.
        comments: The PR's inline review comments.
        login: CodeRabbit's GitHub login.

    Returns:
        A Markdown summary (possibly empty when nothing actionable was found).
    """
    lines: list[str] = []
    failing = [r for r in runs
               if r.get("status") == "completed" and r.get("conclusion") in _FAIL_CONCLUSIONS]
    if failing:
        lines.append("## Failing CI checks")
        for r in failing:
            out = r.get("output") or {}
            summary = (out.get("summary") or out.get("title") or "").strip()
            url = r.get("details_url") or r.get("html_url") or ""
            lines.append(f"- **{r.get('name')}** ({r.get('conclusion')}): {summary} {url}".strip())
    cr_reviews = [rv for rv in reviews
                  if (rv.get("user") or {}).get("login") == login
                  and rv.get("state") == "CHANGES_REQUESTED" and (rv.get("body") or "").strip()]
    cr_comments = [c for c in comments
                   if (c.get("user") or {}).get("login") == login and (c.get("body") or "").strip()]
    if cr_reviews or cr_comments:
        lines.append("\n## CodeRabbit requested changes")
        for rv in cr_reviews:
            lines.append(rv["body"].strip())
        for c in cr_comments:
            loc = c.get("path") or ""
            ln = c.get("line") or c.get("original_line")
            loc = f"`{loc}`:{ln}" if ln is not None else f"`{loc}`"
            lines.append(f"- {loc} — {c['body'].strip()}")
    return "\n".join(lines).strip()
