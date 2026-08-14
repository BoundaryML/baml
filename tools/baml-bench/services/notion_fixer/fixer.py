"""Fix dispatch helpers: choose the target repo by issue kind and format the
@cursor Slack message."""

from __future__ import annotations

import os
from typing import Any

UI_BASE_URL = os.environ.get("UI_BASE_URL", "http://localhost:3000")
CURSOR_BRANCH = os.environ.get("CURSOR_BRANCH", "canary")


def choose_repo(kind: str) -> str:
    """Return the GitHub ``org/repo`` that owns issues of this kind.

    Skill issues live in the SKILL.md repo; language issues in the compiler repo.
    The full org/repo path is returned so Cursor's ``[repo=...]`` directive
    resolves the GitHub repo.

    Args:
        kind: The issue kind, ``"skill"`` or anything else (treated as language).

    Returns:
        The ``org/repo`` slug, e.g. ``"boundaryml/baml"``.
    """
    return "boundaryml/baml-skill" if kind == "skill" else "boundaryml/baml"


def repo_url(kind: str) -> str:
    """Return the full GitHub URL for the repo that owns this issue kind.

    Args:
        kind: The issue kind (see :func:`choose_repo`).

    Returns:
        A ``https://github.com/<org>/<repo>`` URL.
    """
    return f"https://github.com/{choose_repo(kind)}"


def _pr_instructions() -> str:
    """Return the boilerplate telling the agent how to write the pull request.

    The PR is the artifact a human reviews, so it must document the error and the
    fix, not just carry a code diff.

    Returns:
        A multi-line instruction block (error / root cause / fix / verification
        sections plus title guidance).
    """
    return "\n".join([
        "When you open the pull request, write a complete, specific description — "
        "never leave it empty or a single line. Structure it with these sections:",
        "",
        "1. **The error** — paste the failing BAML snippet and the exact error message "
        "or incorrect output it produces. Reuse the reproduction above; if none is "
        "given, construct the smallest BAML that triggers the bug and paste its real output.",
        "2. **Root cause** — why it happens, citing the specific file(s) and function(s) involved.",
        "3. **The fix** — what you changed and why it resolves the error.",
        "4. **Verification** — the exact command(s) you ran and the now-passing output "
        "(show that same reproduction succeeding after the change).",
        "",
        "Give the PR a precise title that names the construct and the behavior "
        '(e.g. "Add `%` modulo operator for integer arithmetic" — not "Fix bug"). '
        "Keep the change minimal and scoped to this single issue.",
    ])


def cursor_prompt(issue: dict[str, Any]) -> str:
    """Build the instruction text for a Cursor cloud agent.

    Repo + branch are passed as separate API params, so this spells out only the
    task AND the PR write-up (see :func:`_pr_instructions`) so the resulting PR
    documents the error and the fix, not just the code change.

    Args:
        issue: The issue document. Uses ``title``, ``category``, ``description``,
            and the optional ``repro`` / ``suggestion`` fields.

    Returns:
        The full prompt string for the agent.
    """
    cat = issue.get("category") or "bug"
    parts = [
        f"Fix this BAML {cat}: {issue['title']}",
        "",
        "## Problem",
        issue.get("description", ""),
    ]
    if issue.get("repro"):
        parts += ["", "## Reproduction (the failing case)", issue["repro"]]
    if issue.get("suggestion"):
        parts += ["", "## Suggested fix", issue["suggestion"]]
    parts += ["", "---", _pr_instructions()]
    return "\n".join(p for p in parts if p is not None)


def evidence_links(issue: dict[str, Any]) -> list[str]:
    """Build dashboard URLs for each trophy cited as evidence on an issue.

    Args:
        issue: The issue document; reads its ``evidence`` list (each entry may
            carry a ``trophyId`` and an optional ``call_index``).

    Returns:
        A list of ``{UI_BASE_URL}/runs/<trophyId>`` URLs (with a ``?call=`` query
        when a call index is present). Evidence entries without a trophy id are
        skipped.
    """
    links = []
    for e in issue.get("evidence") or []:
        tid = e.get("trophyId")
        if not tid:
            continue
        call = e.get("call_index")
        url = f"{UI_BASE_URL}/runs/{tid}"
        if call is not None:
            url += f"?call={call}"
        links.append(url)
    return links

