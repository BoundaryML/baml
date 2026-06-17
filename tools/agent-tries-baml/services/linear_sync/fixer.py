"""Fix dispatch helpers: choose the target repo by issue kind and format the
@cursor Slack message."""

from __future__ import annotations

import os
from typing import Any

UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://new.boundaryml.com/atb")
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
        "Give the PR a precise, descriptive title that names the construct and the "
        'behavior (e.g. "Add `%` modulo operator for integer arithmetic" — NOT "Fix '
        'bug", and NEVER the auto-generated placeholder "Pull request template"). The '
        "PR may be opened automatically with a template/placeholder title; if so, you "
        "MUST overwrite it with your real title — e.g. run "
        '`gh pr edit --title "<precise title>"` after the PR exists. Do not leave a '
        "generic or placeholder title. Keep the change minimal and scoped to this "
        "single issue.",
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


def _language_verification() -> str:
    """Return the pre-PR check that mirrors the compiler repo's CI gates.

    Draft PRs from earlier runs failed CI for two recurring, avoidable reasons,
    both of which "the tests pass" alone does not catch:

    1. **Stale insta snapshots.** Adding a builtin/operator changes captured
       output (e.g. ``baml describe`` listings, bytecode display), so committed
       ``.snap`` files must be regenerated and committed — otherwise the snapshot
       suite fails on every platform.
    2. **Clippy under ``-D warnings``.** The "Pre-commit Checks" job runs clippy
       with warnings denied, so any non-idiomatic lint (e.g.
       ``while_let_on_iterator``) fails CI even when all tests pass.

    This block names both gates and the exact commands so the agent clears them
    locally before opening the draft.

    Returns:
        A multi-line "Before opening the PR" checklist for language fixes.
    """
    return "\n".join([
        "## Before opening the PR — these are the CI gates you MUST clear locally",
        "Run everything below from inside `baml_language/`. \"Tests pass\" is not "
        "enough on its own — past PRs were rejected by CI for the two reasons below "
        "even though the feature worked. Do NOT create the draft until all three pass.",
        "",
        "1. **Full test suite.** Run the whole suite, not just the test for your "
        "change (`cargo test` across the workspace; see "
        "`baml_language/TEST_INSTRUCTIONS.md`). A new builtin/operator can break a "
        "snapshot test in a different crate (e.g. `baml describe` listings or "
        "bytecode display), so a narrow run will miss it.",
        "2. **Snapshot tests (insta).** If any snapshot test fails, it is because "
        "your change altered captured output. Inspect each diff, confirm the new "
        "output is correct, then regenerate and COMMIT the updated snapshots: "
        "`cargo insta accept --all` (and `UPDATE_EXPECT=1 cargo test --package "
        "lsp_actions_tests` for inline expectations). Never leave a `.snap` stale "
        "or delete a failing snapshot test to make it pass.",
        "3. **Lint + format (the \"Pre-commit Checks\" job).** Run `cargo fmt --all` "
        "and `cargo clippy --all-targets -- -D warnings`, and fix every warning. "
        "Clippy runs with warnings DENIED in CI, so a single lint fails the build. "
        "Fix lints idiomatically (e.g. use a `for` loop instead of "
        "`while let Some(..) = iter.next()`); do not silence them with `#[allow(...)]`.",
    ])


def _coding_standards() -> str:
    """Return the code-quality directive requiring docstrings on the diff.

    Reviewers (and CodeRabbit) routinely bounce PRs that add or change public
    items without documenting them. Requiring a docstring on every new or
    modified function/method/type — in the language's idiomatic style — keeps the
    diff self-explanatory and avoids that review round-trip.

    Returns:
        A multi-line "Code standards" instruction block.
    """
    return "\n".join([
        "## Code standards",
        "Every function, method, class/struct/enum, and trait you ADD or MODIFY "
        "must carry a docstring describing what it does — including its parameters, "
        "return value, and any errors/panics. Always include docstrings; do not "
        "leave new or changed public items undocumented.",
        "",
        "- Use the language's idiomatic style: `///` doc comments in Rust, JSDoc "
        "`/** ... */` in TypeScript/JavaScript, `\"\"\"docstrings\"\"\"` in Python.",
        "- Match the surrounding file's existing comment style and density.",
        "- If you touch an existing function that lacks a docstring, add one.",
    ])


def _coderabbit_review(base_branch: str) -> str:
    """Return the step telling the agent to self-review with the CodeRabbit CLI.

    Run after the build/test/lint gates and before the PR is opened: an automated
    CodeRabbit pass over the agent's own diff catches correctness and quality
    issues that a human reviewer would otherwise bounce the PR on. The CLI auths
    non-interactively from ``CODERABBIT_API_KEY`` (provisioned to the Cursor cloud
    agent via the team Secrets store) and reviews the committed diff against the
    repo's base branch.

    Args:
        base_branch: The branch the agent's work is diffed against (the repo's
            starting ref, e.g. ``canary`` for language or ``main`` for skill).

    Returns:
        A multi-line "Self-review with CodeRabbit" instruction block.
    """
    return "\n".join([
        "## Self-review with CodeRabbit before opening the PR",
        "Once the change builds and the checks above pass, review your own diff "
        "with the CodeRabbit CLI and fix what it surfaces — this is a required step.",
        "",
        "1. Install the CLI (skip if `coderabbit` is already on PATH): "
        "`curl -fsSL https://cli.coderabbit.ai/install.sh | sh`",
        "2. Authenticate non-interactively — your environment provides the key as "
        "`CODERABBIT_API_KEY`: `coderabbit auth login --api-key \"$CODERABBIT_API_KEY\"`",
        "3. Commit your work, then review the diff against the base branch: "
        f"`coderabbit review --plain --base {base_branch}`",
        "4. Read every finding. Fix the substantive ones (bugs, correctness, missing "
        "edge cases, security) and re-run the review until it comes back clean. Use "
        "judgment on pure style nits, but do not ignore real issues.",
        "",
        "In the PR's Verification section, note that you ran CodeRabbit and what you "
        "changed in response. If `CODERABBIT_API_KEY` is missing or the CLI cannot "
        "authenticate, say so explicitly in the PR description and proceed — do not "
        "block on it.",
    ])


def _reproduce_first() -> str:
    """Return the directive to reproduce and confirm the issue before fixing it.

    Earlier runs sometimes "fixed" things that weren't actually broken or
    misread the report. Forcing the agent to reproduce the failure first anchors
    the fix to the real behavior, and the captured reproduction is reused as the
    Verification baseline in the PR write-up.

    Returns:
        A multi-line "reproduce and confirm" instruction block.
    """
    return "\n".join([
        "## First: reproduce and confirm the issue",
        "Before changing any code, reproduce the reported problem yourself and "
        "confirm you observe the exact failure described above:",
        "",
        "- Run the reproduction above (or, if none is given, construct the smallest "
        "case that should trigger it) and capture its real output.",
        "- Confirm the actual behavior matches the report — the same error, panic, "
        "or wrong output. Only start fixing once you have reproduced it.",
        "- If you CANNOT reproduce it (it already works, or the report is "
        "inaccurate), do NOT invent a fix. Stop and open the PR describing exactly "
        "what you ran, what you observed, and that the issue does not reproduce.",
        "",
        "Keep this reproduction — you will re-run it in the Verification step to "
        "prove the fix actually resolves it.",
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
    # Always prove the bug exists before touching code (and capture the baseline
    # for the Verification section).
    parts += ["", _reproduce_first()]
    # Require docstrings on everything the agent adds or changes.
    parts += ["", _coding_standards()]
    if issue.get("kind") != "skill":
        # Language fixes go through the compiler repo's CI (tests + insta
        # snapshots + clippy -D warnings); spell out each gate explicitly.
        parts += ["", _language_verification()]
    else:
        parts += [
            "",
            "## Before opening the PR",
            "Run the full test suite and make sure all tests pass. Do not create the PR "
            "draft until every test passes.",
        ]
    # Final gate for every kind: an automated CodeRabbit pass over the agent's own
    # diff, against the same base branch the agent started from.
    parts += ["", _coderabbit_review(branch_for(issue.get("kind", "")))]
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


def issue_link(issue: dict[str, Any]) -> str:
    """Dashboard URL for an issue's own page (``{UI_BASE_URL}/issues/<id>``).

    Args:
        issue: The issue document; reads its Convex ``_id``.

    Returns:
        The issue's dashboard URL, or "" when the issue has no id.
    """
    iid = issue.get("_id")
    return f"{UI_BASE_URL}/issues/{iid}" if iid else ""

