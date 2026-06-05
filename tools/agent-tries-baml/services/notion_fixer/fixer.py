"""Fix dispatch helpers: choose the target repo by issue kind and format the
@cursor Slack message."""

from __future__ import annotations

import os
from typing import Any

UI_BASE_URL = os.environ.get("UI_BASE_URL", "http://localhost:3000")
# Starting branch per repo: the compiler repo (language issues) ships on canary,
# the skill repo (skill issues) on main. Cursor verifies the ref exists, so each
# must match its repo's actual default branch.
CURSOR_BRANCH = os.environ.get("CURSOR_BRANCH", "canary")
CURSOR_BRANCH_SKILL = os.environ.get("CURSOR_BRANCH_SKILL", "main")


def branch_for(kind: str) -> str:
    """Return the starting git ref for the repo that owns this issue kind.

    Args:
        kind: The issue kind, ``"skill"`` or anything else (treated as language).

    Returns:
        ``"main"`` for the skill repo, otherwise ``"canary"`` for the compiler repo.
    """
    return CURSOR_BRANCH_SKILL if kind == "skill" else CURSOR_BRANCH


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


def _language_workspace_note() -> str:
    """Return the directive pinning language fixes to the ``baml_language/`` workspace.

    The compiler repo carries two Rust implementations: the legacy ``engine/``
    tree and the current ``baml_language/`` workspace (compiler2). They look
    interchangeable from the repo root, so an agent left to its own devices will
    often "fix" the issue in ``engine/`` — the wrong place. This spells out that
    the live compiler is ``baml_language/`` and ``engine/`` must not be touched.

    Returns:
        A multi-line instruction block scoping the work to ``baml_language/``.
    """
    return "\n".join([
        "## Where to work",
        "This issue is in the **current BAML compiler, which lives in the "
        "`baml_language/` directory** of this repo (the compiler2 workspace — its "
        "crates are under `baml_language/crates/`, see `baml_language/ARCHITECTURE.md`).",
        "",
        "- Make ALL of your changes inside `baml_language/`.",
        "- Do NOT modify the legacy `engine/` directory — it is the old "
        "implementation and is not what this issue is about. A fix in `engine/` "
        "is in the wrong place and will be rejected.",
        "- Build and run the tests from within `baml_language/`.",
    ])


def cursor_prompt(issue: dict[str, Any]) -> str:
    """Build the instruction text for a Cursor cloud agent.

    Repo + branch are passed as separate API params, so this spells out only the
    task AND the PR write-up (see :func:`_pr_instructions`) so the resulting PR
    documents the error and the fix, not just the code change. Language issues
    also get a :func:`_language_workspace_note` pinning the fix to the
    ``baml_language/`` workspace (skill issues live in a different repo entirely,
    so the note is omitted for them).

    Args:
        issue: The issue document. Uses ``title``, ``category``, ``kind``,
            ``description``, and the optional ``repro`` / ``suggestion`` fields.

    Returns:
        The full prompt string for the agent.
    """
    cat = issue.get("category") or "bug"
    parts = [
        f"Fix this BAML {cat}: {issue['title']}",
    ]
    if issue.get("kind") != "skill":
        parts += ["", _language_workspace_note()]
    parts += [
        "",
        "## Problem",
        issue.get("description", ""),
    ]
    if issue.get("repro"):
        parts += ["", "## Reproduction (the failing case)", issue["repro"]]
    if issue.get("suggestion"):
        parts += ["", "## Suggested fix", issue["suggestion"]]
    parts += [
        "",
        "## Before opening the PR",
        "Run the full test suite and make sure all tests pass. Do not create the PR "
        "draft until every test passes.",
    ]
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

