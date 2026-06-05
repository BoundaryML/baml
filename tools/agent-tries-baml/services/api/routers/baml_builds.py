"""baml version registry + build coordination endpoints (multi-channel).

`POST /baml/update?channel=nightly|canary` resolves the latest BAML release on
the given channel — **nightly** (`baml-language-*-nightly.*` pre-releases) or
**canary** (the plain stable `baml-language-X.Y.Z` releases) — and enqueues
exactly one build per release. The builder downloads that release's binary; the
build row stores the release tag in `ref`, the channel in `channel`, and the
release commit in `sha` (the proxy/blob cache key). Proxies pull the binary by sha.

`GET /baml/status/{sha}` reports a single build's status so a run can block until
its pinned build is ready. `POST /baml/prune` keeps only the newest
``BAML_KEEP_RELEASES`` ready builds **per tracked channel** in the builder bucket,
deleting older binaries and their rows (and any untracked legacy builds).
"""

from __future__ import annotations

import os
from typing import Any, Optional

import httpx
from fastapi import APIRouter, HTTPException, Query, Request, Response

from bench_core.channels import DEFAULT_CHANNEL, TRACKED_CHANNELS, channel_of_tag

from ..convex_gateway import ConvexGateway
from .. import blobs

BAML_REPO_SLUG = os.environ.get("BAML_REPO_SLUG", "BoundaryML/baml")
# How many ready builds to retain in the builder bucket per channel; older ones
# are pruned.
BAML_KEEP_RELEASES = int(os.environ.get("BAML_KEEP_RELEASES", "5"))
# GitHub API base; env-overridable for consistency with the other API clients.
GITHUB_API_BASE = os.environ.get("GITHUB_API_BASE", "https://api.github.com")


def _gh_headers(accept: str = "application/vnd.github+json") -> dict[str, str]:
    """Build GitHub API request headers, adding auth when available.

    Args:
        accept: Value for the Accept header.

    Returns:
        A headers dict including a bearer token when GITHUB_TOKEN is set.
    """
    h = {"Accept": accept}
    token = os.environ.get("ATB_GITHUB_TOKEN")
    if token:
        h["Authorization"] = f"Bearer {token}"
    return h


async def _resolve_release(slug: str, channel: str) -> tuple[str, str]:
    """Resolve the latest baml-language release on a channel from GitHub.

    Scans the repo's recent releases for the newest one whose tag classifies as
    ``channel`` (see ``bench_core.channels.channel_of_tag``): ``nightly`` matches
    the ``*-nightly.*`` pre-releases, ``canary`` matches the plain stable
    ``baml-language-X.Y.Z`` releases. Resolves the tag to a 40-char commit sha
    when necessary — both lines carry ``target_commitish: canary`` (a branch, not
    a sha), so the ``/commits/{tag}`` fallback below resolves the real commit.

    Args:
        slug: GitHub ``owner/repo`` slug to query.
        channel: Channel to resolve (``nightly`` or ``canary``).

    Returns:
        A tuple of (commit_sha, release_tag).

    Raises:
        HTTPException: 502 when no matching release is found on the channel.
        httpx.HTTPStatusError: When a GitHub request fails.
    """
    async with httpx.AsyncClient(timeout=25.0) as c:
        r = await c.get(
            f"{GITHUB_API_BASE}/repos/{slug}/releases?per_page=60", headers=_gh_headers()
        )
        r.raise_for_status()
        rels = r.json()
        matches = [
            x for x in rels
            if not x.get("draft") and channel_of_tag(x.get("tag_name", "")) == channel
        ]
        if not matches:
            raise HTTPException(502, f"no {channel} release found")
        tag = matches[0]["tag_name"]  # releases come newest-first
        sha = matches[0].get("target_commitish") or ""
        is_sha = len(sha) == 40 and all(ch in "0123456789abcdef" for ch in sha.lower())
        if not is_sha:
            rr = await c.get(
                f"{GITHUB_API_BASE}/repos/{slug}/commits/{tag}",
                headers=_gh_headers("application/vnd.github.sha"),
            )
            rr.raise_for_status()
            sha = rr.text.strip()
    return sha, tag


def make_baml_router(convex: ConvexGateway) -> APIRouter:
    """Build the baml version registry and build-coordination router.

    Args:
        convex: Gateway used to read/write the ``bamlBuilds`` table.

    Returns:
        An APIRouter exposing the baml current/update/binary endpoints.
    """
    r = APIRouter(tags=["baml"])

    @r.get("/baml/current")
    async def current() -> dict[str, Any]:
        """Return metadata for the newest ready baml build.

        Returns:
            A dict describing the current build: sha, ref/version, builtAt,
            sizeBytes, and contentHash.

        Raises:
            HTTPException: 404 when no ready build exists yet.
        """
        ready = await convex.query(
            "bamlBuilds:list",
            {"field": "status", "value": "ready", "index": "by_status_created", "limit": 50},
        )
        if ready:  # newest ready build (its `ref` is the nightly release tag)
            row = ready[0]
            return {
                "sha": row["sha"],
                "ref": row.get("ref"),
                "version": row.get("ref"),
                "builtAt": row.get("builtAt"),
                "sizeBytes": row.get("sizeBytes"),
                "contentHash": row.get("contentHash"),
            }
        raise HTTPException(404, "no ready baml build yet")

    @r.post("/baml/update")
    async def update(channel: str = Query(default=DEFAULT_CHANNEL)) -> dict[str, Any]:
        """Resolve the latest release on a channel and enqueue a build if needed.

        Idempotent per release sha: returns early when a build for the sha is
        already ready or in flight, otherwise enqueues exactly one build with its
        ``channel`` recorded.

        Args:
            channel: Release channel to refresh (``nightly`` or ``canary``).

        Returns:
            A dict reporting whether the build is already built or newly
            enqueued, including the sha, release version, and channel.

        Raises:
            HTTPException: 400 for an unknown channel.
        """
        if channel not in TRACKED_CHANNELS:
            raise HTTPException(400, f"unknown channel {channel!r}")
        sha, tag = await _resolve_release(BAML_REPO_SLUG, channel)
        existing = await convex.query("bamlBuilds:list", {"field": "sha", "value": sha,
                                                           "index": "by_sha", "limit": 5})
        for row in existing:
            if row.get("status") == "ready":
                return {"built": True, "sha": sha, "version": tag, "channel": channel}
            if row.get("status") in ("queued", "building"):
                return {"built": False, "enqueued": sha, "version": tag,
                        "channel": channel, "pending": True}
        await convex.mutation(
            "bamlBuilds:create",
            {"doc": {"sha": sha, "ref": tag, "channel": channel, "status": "queued"}},
        )
        return {"built": False, "enqueued": sha, "version": tag, "channel": channel}

    @r.get("/baml/status/{sha}")
    async def status(sha: str) -> dict[str, Any]:
        """Report the build status for a single release sha.

        Lets a run block until its pinned nightly is ready (or observe that it
        failed) by polling this endpoint.

        Args:
            sha: Release commit sha to look up.

        Returns:
            A dict ``{sha, status}`` where status is the build's status
            (queued | building | ready | failed) or ``"missing"`` when no
            build row exists for the sha.
        """
        rows = await convex.query(
            "bamlBuilds:list",
            {"field": "sha", "value": sha, "index": "by_sha", "limit": 5},
        )
        return {"sha": sha, "status": rows[0]["status"] if rows else "missing"}

    @r.post("/baml/prune")
    async def prune() -> dict[str, Any]:
        """Retain only the newest ``BAML_KEEP_RELEASES`` ready builds per channel.

        Lists ready builds newest-first, groups them by channel (taken from the
        row or derived from its ref), and keeps the configured number per *tracked*
        channel. Everything else — older-than-N in a channel and any untracked
        (legacy alpha) build — has its binary and row deleted.

        Returns:
            A dict with per-channel kept counts and the total deleted.
        """
        ready = await convex.query(
            "bamlBuilds:list",
            {"field": "status", "value": "ready", "index": "by_status_created",
             "limit": 1000},
        )
        kept: dict[str, int] = {}
        deleted = 0
        for row in ready:  # newest-first
            ch = row.get("channel") or channel_of_tag(row.get("ref"))
            keep = ch in TRACKED_CHANNELS and kept.get(ch, 0) < BAML_KEEP_RELEASES
            if keep:
                kept[ch] = kept.get(ch, 0) + 1
                continue
            storage_id = row.get("binaryStorageId") or f"baml/{row['sha']}"
            blobs.delete_binary(storage_id)
            await convex.mutation("bamlBuilds:remove", {"id": row["_id"]})
            deleted += 1
        return {"kept": kept, "deleted": deleted}

    @r.post("/baml-builds/{build_id}/binary")
    async def upload_binary(build_id: str, request: Request) -> dict[str, Any]:
        """Store a built baml binary and record its pointer on the build row.

        Args:
            build_id: Convex document id of the ``bamlBuilds`` row.
            request: Request whose raw body is the binary payload.

        Returns:
            A dict with the storage id, content hash, and size in bytes.

        Raises:
            HTTPException: 404 when the build row does not exist.
        """
        doc = await convex.query("bamlBuilds:get", {"id": build_id})
        if doc is None:
            raise HTTPException(404, "build not found")
        data = await request.body()
        storage_id, digest, size = blobs.put_binary("baml", doc["sha"], data)
        await convex.mutation(
            "bamlBuilds:update",
            {"id": build_id, "patch": {"binaryStorageId": storage_id,
                                       "contentHash": digest, "sizeBytes": size}},
        )
        return {"storageId": storage_id, "contentHash": digest, "sizeBytes": size}

    @r.get("/baml/binary/{sha}")
    async def download_binary(sha: str) -> Response:
        """Serve a built baml binary by its release sha.

        Args:
            sha: Release commit sha used as the binary's blob key.

        Returns:
            An application/octet-stream Response containing the binary.

        Raises:
            HTTPException: 404 when no binary is stored for the sha.
        """
        storage_id = f"baml/{sha}"
        if not blobs.exists(storage_id):
            raise HTTPException(404, "binary not found for sha")
        return Response(content=blobs.get_binary(storage_id),
                        media_type="application/octet-stream")

    return r
