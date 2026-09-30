"""Dump a GitHub PR with `gh` and render a plaintext copy of it.

    uv run python seed.py            # BoundaryML/baml#5041
    uv run python seed.py 1234       # any other PR in the current repo

Writes `data/pr_<n>.json` (the ground truth, shaped like `baml_src/pull_request.prompt.baml`)
and `data/pr_<n>.txt` (the same data as prose, which the models extract from).
"""

import json
import subprocess
import sys
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"

# Every scalar/list field `gh pr view --json` offers that describes the PR itself.
FIELDS = [
    "number",
    "title",
    "url",
    "state",
    "isDraft",
    "author",
    "createdAt",
    "mergedAt",
    "closedAt",
    "baseRefName",
    "headRefName",
    "labels",
    "additions",
    "deletions",
    "changedFiles",
    "body",
    "files",
    "commits",
    "reviews",
    "comments",
]

# Viewer-specific or always-empty noise; dropped so the JSON matches the BAML type.
DROP_KEYS = {"reactionGroups", "viewerDidAuthor", "includesCreatedEdit", "isMinimized", "minimizedReason"}


def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in DROP_KEYS}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def dump_pr(number: int, repo: str) -> dict:
    out = subprocess.run(
        ["gh", "pr", "view", str(number), "--repo", repo, "--json", ",".join(FIELDS)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return strip(json.loads(out))


def _or_dash(v) -> str:
    return "-" if v in (None, "") else str(v)


def _yes_no(v: bool) -> str:
    return "yes" if v else "no"


def _author(a: dict) -> str:
    parts = [f"@{a['login']}"]
    if a.get("name"):
        parts.insert(0, a["name"])
    extras = []
    if a.get("id"):
        extras.append(f"id {a['id']}")
    if "is_bot" in a:
        extras.append(f"bot: {_yes_no(a['is_bot'])}")
    return " ".join(parts) + (f" ({', '.join(extras)})" if extras else "")


def _indent(text: str, prefix: str = "    ") -> str:
    return "\n".join(prefix + line if line else line for line in text.splitlines())


def to_plaintext(pr: dict) -> str:
    """Prose rendering: every value in `pr` appears once, with no JSON syntax."""
    lines = [
        f"Pull request #{pr['number']}: {pr['title']}",
        f"URL: {pr['url']}",
        f"State: {pr['state']} (draft: {_yes_no(pr['isDraft'])})",
        f"Opened by {_author(pr['author'])}",
        f"Created at {pr['createdAt']}; merged at {_or_dash(pr['mergedAt'])}; closed at {_or_dash(pr['closedAt'])}",
        f"Merging {pr['headRefName']} into {pr['baseRefName']}",
        f"Diff: {pr['additions']} additions and {pr['deletions']} deletions across {pr['changedFiles']} changed files",
        "Labels: " + (", ".join(label["name"] for label in pr["labels"]) or "none"),
        "",
        "Description:",
        _indent(pr["body"]),
        "",
        f"Files changed ({len(pr['files'])}):",
    ]
    for f in pr["files"]:
        lines.append(f"  - {f['path']} was {f['changeType'].lower()} (+{f['additions']} / -{f['deletions']})")

    lines += ["", f"Commits ({len(pr['commits'])}):"]
    for i, c in enumerate(pr["commits"], 1):
        authors = "; ".join(f"{a['name']} <{a['email']}> (@{a['login']}, id {_or_dash(a['id'])})" for a in c["authors"])
        lines += [
            f"  {i}. Commit {c['oid']}: {c['messageHeadline']}",
            f"     Authored at {c['authoredDate']}, committed at {c['committedDate']}",
            f"     Authors: {authors}",
            "     Message body:",
            _indent(c["messageBody"], "       "),
        ]

    lines += ["", f"Reviews ({len(pr['reviews'])}):"]
    for i, r in enumerate(pr["reviews"], 1):
        lines += [
            f"  {i}. Review {r['id']} by @{r['author']['login']} ({r['authorAssociation']}), state {r['state']}, "
            f"submitted at {r['submittedAt']} on commit {r['commit']['oid']}",
            "     Body:",
            _indent(r["body"], "       "),
        ]

    lines += ["", f"Comments ({len(pr['comments'])}):"]
    for i, c in enumerate(pr["comments"], 1):
        lines += [
            f"  {i}. Comment {c['id']} by @{c['author']['login']} ({c['authorAssociation']}) at {c['createdAt']}",
            f"     Link: {c['url']}",
            "     Body:",
            _indent(c["body"], "       "),
        ]
    return "\n".join(lines) + "\n"


def main() -> None:
    number = int(sys.argv[1]) if len(sys.argv) > 1 else 5041
    repo = sys.argv[2] if len(sys.argv) > 2 else "BoundaryML/baml"
    pr = dump_pr(number, repo)
    DATA_DIR.mkdir(exist_ok=True)
    (DATA_DIR / f"pr_{number}.json").write_text(json.dumps(pr, indent=2, ensure_ascii=False) + "\n")
    (DATA_DIR / f"pr_{number}.txt").write_text(to_plaintext(pr))
    print(f"wrote data/pr_{number}.json and data/pr_{number}.txt")


if __name__ == "__main__":
    main()
