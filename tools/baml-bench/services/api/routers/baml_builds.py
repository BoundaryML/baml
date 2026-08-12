"""baml version registry + build coordination endpoints.

`POST /baml/update` resolves the latest BAML *alpha-channel* release (the
GitHub pre-releases named `baml-language-*-alpha.*`) and enqueues exactly
one build per release. The builder downloads that release's binary; the
build row stores the release tag in `ref` and the release commit in `sha`
(the proxy/blob cache key). Proxies pull the binary by sha.
"""

from __future__ import annotations

import os
from typing import Any, Optional

import httpx
from fastapi import APIRouter, HTTPException, Request, Response

from ..convex_gateway import ConvexGateway
from .. import blobs

BAML_REPO_SLUG = os.environ.get("BAML_REPO_SLUG", "BoundaryML/baml")
# Which release channel to track. "alpha" = latest baml-language alpha pre-release.
BAML_CHANNEL = os.environ.get("BAML_CHANNEL", "alpha")
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
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        h["Authorization"] = f"Bearer {token}"
    return h


async def _resolve_alpha(slug: str) -> tuple[str, str]:
    """Resolve the latest baml-language alpha pre-release from GitHub.

    Scans the repo's recent releases for the newest ``baml-language*`` alpha
    pre-release, resolving its tag to a 40-char commit sha when necessary.

    Args:
        slug: GitHub ``owner/repo`` slug to query.

    Returns:
        A tuple of (commit_sha, release_tag).

    Raises:
        HTTPException: 502 when no matching alpha pre-release is found.
        httpx.HTTPStatusError: When a GitHub request fails.
    """
    async with httpx.AsyncClient(timeout=25.0) as c:
        r = await c.get(
            f"{GITHUB_API_BASE}/repos/{slug}/releases?per_page=30", headers=_gh_headers()
        )
        r.raise_for_status()
        rels = r.json()
        alphas = [
            x for x in rels
            if x.get("prerelease") and "alpha" in x.get("tag_name", "")
            and x["tag_name"].startswith("baml-language")
        ]
        if not alphas:
            raise HTTPException(502, "no alpha pre-release found")
        tag = alphas[0]["tag_name"]  # releases come newest-first
        sha = alphas[0].get("target_commitish") or ""
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
        if ready:  # newest ready build (its `ref` is the alpha release tag)
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
    async def update() -> dict[str, Any]:
        """Resolve the latest alpha release and enqueue a build if needed.

        Idempotent per release sha: returns early when a build for the sha is
        already ready or in flight, otherwise enqueues exactly one build.

        Returns:
            A dict reporting whether the build is already built or newly
            enqueued, including the sha and release version.
        """
        sha, tag = await _resolve_alpha(BAML_REPO_SLUG)
        existing = await convex.query("bamlBuilds:list", {"field": "sha", "value": sha,
                                                           "index": "by_sha", "limit": 5})
        for row in existing:
            if row.get("status") == "ready":
                return {"built": True, "sha": sha, "version": tag}
            if row.get("status") in ("queued", "building"):
                return {"built": False, "enqueued": sha, "version": tag, "pending": True}
        await convex.mutation(
            "bamlBuilds:create",
            {"doc": {"sha": sha, "ref": tag, "status": "queued"}},
        )
        return {"built": False, "enqueued": sha, "version": tag}

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
