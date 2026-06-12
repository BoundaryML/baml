"""Serve and accept the prebuilt bridge_wasm tarball (absorbs baml-wasm-service).

The website's vercel-build.sh downloads GET /wasm/bridge_wasm.tar.gz at deploy
time instead of compiling Rust on Vercel. scripts/publish_wasm.sh builds the
tarball from the monorepo and PUTs it here (bearer-authed); the GET is public
and must be registered WITHOUT the service-token dependency in app.py.

The artifact lives on the api's blob volume (not baked into the image) so a
wasm publish never forces an api redeploy.
"""

from __future__ import annotations

import io
import os
import tarfile

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import FileResponse

from .. import blobs

WASM_STORAGE_ID = "wasm/bridge_wasm.tar.gz"
_MAX_UPLOAD_BYTES = 100 * 1024 * 1024  # generous; the tarball is ~10MB today


def _validate_tarball(data: bytes) -> None:
    """Reject uploads that aren't a bridge_wasm tarball.

    Args:
        data: The raw upload bytes.

    Raises:
        HTTPException: 400 when the payload is not a gzip tar containing
            SOURCE_HASH and the wasm binary.
    """
    try:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tf:
            names = {os.path.basename(n) for n in tf.getnames()}
    except (tarfile.TarError, OSError, EOFError) as e:
        raise HTTPException(400, f"not a valid gzip tarball: {e}")
    required = {"SOURCE_HASH", "bridge_wasm_bg.wasm"}
    missing = required - names
    if missing:
        raise HTTPException(400, f"tarball missing expected members: {sorted(missing)}")


def make_wasm_public_router() -> APIRouter:
    """Build the public (unauthenticated) wasm download router.

    Returns:
        An APIRouter exposing GET /wasm/bridge_wasm.tar.gz.
    """
    r = APIRouter(tags=["wasm"])

    @r.get("/wasm/bridge_wasm.tar.gz")
    async def get_tarball() -> FileResponse:
        """Serve the current bridge_wasm tarball with website cache headers.

        Returns:
            The tarball as application/gzip with a 5-minute public cache.

        Raises:
            HTTPException: 404 when no tarball has been published yet.
        """
        if not blobs.exists(WASM_STORAGE_ID):
            raise HTTPException(404, "no wasm artifact published")
        return FileResponse(
            blobs._path(WASM_STORAGE_ID),
            media_type="application/gzip",
            headers={"Cache-Control": "public, max-age=300"},
        )

    return r


def make_wasm_upload_router() -> APIRouter:
    """Build the bearer-authed wasm upload router.

    Returns:
        An APIRouter exposing PUT /wasm/bridge_wasm.tar.gz.
    """
    r = APIRouter(tags=["wasm"])

    @r.put("/wasm/bridge_wasm.tar.gz")
    async def put_tarball(request: Request) -> dict[str, int]:
        """Replace the published tarball (validate, then atomic rename).

        Args:
            request: Request whose raw body is the gzip tarball.

        Returns:
            A dict with the stored ``sizeBytes``.

        Raises:
            HTTPException: 400 on invalid payloads, 413 when oversized.
        """
        data = await request.body()
        if len(data) > _MAX_UPLOAD_BYTES:
            raise HTTPException(413, "tarball too large")
        if not data:
            raise HTTPException(400, "empty body")
        _validate_tarball(data)
        # Write beside the live file, then atomically swap so a concurrent
        # GET never sees a partial artifact.
        tmp_id = WASM_STORAGE_ID + ".tmp"
        blobs.put_binary("wasm", "bridge_wasm.tar.gz.tmp", data)
        os.replace(blobs._path(tmp_id), blobs._path(WASM_STORAGE_ID))
        return {"sizeBytes": len(data)}

    return r
