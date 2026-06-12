"""Read-only GitHub helpers for the BAML repo (ported from baml-changelog2).

Given a release tag the changelog worker fetches: the release itself (for the date),
its predecessor on the same channel (for the compare base), and the list of
commits between the two with author info. All of this becomes the context for
the Anthropic call.

Auth: optional `GITHUB_TOKEN` env. Unauthenticated requests are rate-limited
to 60/hr per IP; authenticated to 5000/hr.
"""

from __future__ import annotations

import os
import re
from typing import Any

import httpx

REPO = os.environ.get("BAML_REPO", "boundaryml/baml")
API = "https://api.github.com"

# Release channels on boundaryml/baml. Two live channels today:
#   nightly -> baml-language-X.Y.Z-nightly.YYYYMMDD.<letter>  (frequent, auto)
#   canary  -> baml-language-X.Y.Z                            (cut from a nightly)
# Legacy channels (no longer produced, kept so historical POSTs do not 500):
#   alpha   -> baml-language-X.Y.Z-alpha.N
#   engine  -> X.Y.Z or v-prefixed
_NIGHTLY_RE = re.compile(
    r"^baml-language-(\d+\.\d+\.\d+-nightly\.\d{8}\.[a-z0-9]+)$"
)
_CANARY_RE = re.compile(r"^baml-language-(\d+\.\d+\.\d+)$")
_ALPHA_RE = re.compile(r"^baml-language-(\d+\.\d+\.\d+-alpha\.\d+)$")
_ENGINE_RE = re.compile(r"^v?\d+\.\d+\.\d+$")


class GitHubError(RuntimeError):
    """Raised for any non-2xx response or transport error."""


def _headers() -> dict[str, str]:
    h = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
    }
    tok = os.environ.get("GITHUB_TOKEN")
    if tok:
        h["Authorization"] = f"Bearer {tok}"
    return h


def channel_of(tag: str) -> str | None:
    """The release channel this tag belongs to, or None for unrelated tags
    (e.g. `baml-wrapper-*`)."""
    if _NIGHTLY_RE.match(tag):
        return "nightly"
    if _CANARY_RE.match(tag):
        return "canary"
    if _ALPHA_RE.match(tag):
        return "alpha"
    if _ENGINE_RE.match(tag):
        return "engine"
    return None


# Back-compat alias for the private helper name used elsewhere in this file.
_channel = channel_of


def normalize(tag: str) -> str:
    """Strip the `baml-language-` prefix (and a `v` prefix on engine tags) so the
    UI shows clean version strings."""
    for r in (_NIGHTLY_RE, _CANARY_RE, _ALPHA_RE):
        m = r.match(tag)
        if m:
            return m.group(1)
    return tag[1:] if tag.startswith("v") else tag


def to_tag(version: str, channel: str | None) -> str:
    """Reverse of `normalize`: reconstruct the GitHub tag from the stored
    (normalized) version and its channel, so we can re-fetch the diff for an
    existing entry.

    nightly / canary / alpha tags are `baml-language-<version>` — the nightly
    date/letter suffix and the plain canary `X.Y.Z` already live inside the
    normalized version (see `_NIGHTLY_RE` / `_CANARY_RE`), so a single prefix
    re-add round-trips exactly. engine tags may or may not carry a `v`, so try
    the bare form first and let the caller fall back. unknown returns the
    version verbatim."""
    if channel in ("nightly", "canary", "alpha"):
        return f"baml-language-{version}"
    # engine / unknown: the version is already the tag (or a v-stripped tag).
    return version


def _get(client: httpx.Client, path: str, **params: Any) -> Any:
    r = client.get(f"{API}{path}", headers=_headers(), params=params or None)
    if r.status_code == 404:
        raise GitHubError(f"not found: {path}")
    if r.status_code >= 400:
        raise GitHubError(f"{r.status_code} {path}: {r.text[:200]}")
    return r.json()


def get_release(tag: str) -> dict[str, Any]:
    with httpx.Client(timeout=30) as c:
        return _get(c, f"/repos/{REPO}/releases/tags/{tag}")


def _list_releases(max_pages: int = 3, per_page: int = 100) -> list[dict[str, Any]]:
    """All non-draft releases, ordered by created_at descending (GitHub default)."""
    out: list[dict[str, Any]] = []
    with httpx.Client(timeout=30) as c:
        for page in range(1, max_pages + 1):
            batch = _get(c, f"/repos/{REPO}/releases", per_page=per_page, page=page)
            if not batch:
                break
            out.extend(batch)
    return [r for r in out if not r.get("draft")]


def previous_release(tag: str) -> str | None:
    """The tag of the release immediately before `tag` on the same channel.

    Uses GitHub's `created_at` for ordering, which is correct across all channels
    (nightlies' letter suffix, stable cuts from a nightly, etc.). Returns None
    when `tag` is the oldest release on its channel or the tag is on an unknown
    channel.
    """
    channel = _channel(tag)
    if channel is None:
        return None

    rels = _list_releases()
    target = next((r for r in rels if r["tag_name"] == tag), None)
    if target is None:
        return None

    target_ts = target.get("created_at") or ""
    same_channel = [
        r
        for r in rels
        if _channel(r["tag_name"]) == channel
        and (r.get("created_at") or "") < target_ts
    ]
    if not same_channel:
        return None
    # `_list_releases` is newest-first, so the first same-channel entry strictly
    # older than the target IS the immediate predecessor.
    return same_channel[0]["tag_name"]


def recent_release_tags(
    channels: tuple[str, ...] = ("nightly", "canary"), limit: int = 20
) -> list[str]:
    """The most recent N release tags across the given channels, newest-first."""
    rels = _list_releases()
    tags: list[str] = []
    for r in rels:
        if _channel(r["tag_name"]) in channels:
            tags.append(r["tag_name"])
            if len(tags) >= limit:
                break
    return tags


def _compare(base: str, head: str) -> dict[str, Any]:
    with httpx.Client(timeout=60) as c:
        return _get(c, f"/repos/{REPO}/compare/{base}...{head}")


# Files we never want in the prompt (lockfiles, generated code, binaries,
# snapshots). Everything else is fair game up to the char budget.
_SKIP_FILE_RE = re.compile(
    r"\.(lock|snap|min\.js|map|svg|png|jpg|jpeg|gif|ico|woff2?|ttf|otf|pdf|zip)$"
    r"|(^|/)(node_modules|dist|build|out|target|\.next|_generated|__snapshots__)(/|$)"
    r"|(^|/)package-lock\.json$"
)


def _diff_for_prompt(cmp: dict[str, Any], budget_chars: int = 80000) -> str:
    """Render the compare API's file patches as a single concatenated diff, up
    to `budget_chars`. The model gets the ACTUAL code that moved — not just
    commit messages — so the draft can be specific instead of hallucinating."""
    parts: list[str] = []
    used = 0
    files = cmp.get("files") or []
    for f in files:
        name = f.get("filename", "")
        if _SKIP_FILE_RE.search(name):
            continue
        patch = f.get("patch") or ""
        if not patch:
            continue
        # Keep diversity: cap each file's patch so one huge file does not
        # consume the entire budget.
        if len(patch) > 6000:
            patch = patch[:6000] + "\n[...patch truncated...]"
        header = (
            f"\n=== {name} "
            f"({f.get('status', '?')}: "
            f"+{f.get('additions', 0)}/-{f.get('deletions', 0)}) ===\n"
        )
        block = header + patch + "\n"
        if used + len(block) > budget_chars:
            parts.append(
                f"\n[remaining {len(files) - files.index(f)} files omitted; "
                f"diff truncated at ~{budget_chars} chars]\n"
            )
            break
        parts.append(block)
        used += len(block)
    return "".join(parts)


def collect_context(tag: str, base_tag: str | None) -> dict[str, Any]:
    """Build the full release context for the prompt.

    Returns: { version, from_version, date, commit_log, authors }
    where `version` and `from_version` are NORMALIZED (no `baml-language-` or
    `v` prefix), `date` is YYYY-MM-DD, `commit_log` is one line per commit, and
    `authors` is deduplicated GitHub logins (or names) with bot accounts dropped.
    """
    rel = get_release(tag)
    date = (rel.get("published_at") or rel.get("created_at") or "")[:10]

    if base_tag is None:
        base_tag = previous_release(tag)

    commits: list[str] = []
    authors: list[str] = []
    diff = ""

    if base_tag:
        cmp = _compare(base_tag, tag)
        for entry in cmp.get("commits", []):
            sha = (entry.get("sha") or "")[:7]
            msg = ((entry.get("commit") or {}).get("message") or "").splitlines()
            subject = msg[0] if msg else ""
            login = (entry.get("author") or {}).get("login") or ""
            name = ((entry.get("commit") or {}).get("author") or {}).get("name") or ""
            who = login or name
            if who and who not in authors and not who.endswith("[bot]"):
                authors.append(who)
            line = f"{sha} {subject}"
            if who:
                line += f" ({who})"
            commits.append(line)
            if len(commits) >= 200:  # keep the prompt sane on huge ranges
                break
        diff = _diff_for_prompt(cmp)

    return {
        "version": normalize(tag),
        "from_version": normalize(base_tag) if base_tag else None,
        "date": date,
        "channel": channel_of(tag),
        "commit_log": "\n".join(commits),
        "authors": authors,
        "diff": diff,
    }
