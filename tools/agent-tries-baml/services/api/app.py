"""agent-tries-baml API - the central HTTP API and sole Convex gateway.

Exposes uniform CRUD + queue verbs + SSE per table (tasks, trophies,
issues, bamlBuilds), the baml version endpoints, and transcript blobs.
Bearer-token auth on every route.
"""

from __future__ import annotations

import hmac
import os

from fastapi import Depends, FastAPI, Header, HTTPException, Request, Response

from .convex_gateway import gateway_from_env
from . import blobs
from .routers.table import make_router
from .routers.baml_builds import make_baml_router
from .routers.ingest import make_ingest_router
from .routers.promo import make_promo_router
from .routers.workers import make_workers_router
from .routers.wasm import make_wasm_public_router, make_wasm_upload_router

SERVICE_TOKEN = os.environ.get("ATB_SERVICE_TOKEN", "")
TABLES = ["tasks", "trophies", "issues", "bamlBuilds", "cohorts", "changelogEntries"]


async def require_bearer(authorization: str = Header(default="")) -> None:
    """Enforce bearer-token auth on a request.

    No-op when no SERVICE_TOKEN is configured (dev mode); otherwise the
    Authorization header must match ``Bearer <SERVICE_TOKEN>``.

    Args:
        authorization: The request's Authorization header value.

    Raises:
        HTTPException: 401 when the token is missing or does not match.
    """
    if not SERVICE_TOKEN:
        return  # dev mode: no token configured
    expected = f"Bearer {SERVICE_TOKEN}"
    if not hmac.compare_digest(authorization or "", expected):
        raise HTTPException(401, "unauthorized")


def create_app() -> FastAPI:
    """Build and return the configured FastAPI application.

    Wires up the Convex gateway, the per-table CRUD/queue routers, the baml
    router, the transcript blob endpoint, and bearer-token auth.

    Returns:
        The fully configured FastAPI app.
    """
    convex = gateway_from_env()
    app = FastAPI(title="agent-tries-baml-api", on_shutdown=[convex.aclose])
    auth = [Depends(require_bearer)]

    @app.get("/healthz")
    async def healthz() -> str:
        """Report service liveness for health checks.

        Returns:
            The literal string ``"ok"``.
        """
        return "ok"

    for table in TABLES:
        app.include_router(make_router(table, convex), dependencies=auth)
    app.include_router(make_baml_router(convex), dependencies=auth)
    app.include_router(make_ingest_router(convex), dependencies=auth)
    app.include_router(make_promo_router(convex), dependencies=auth)
    app.include_router(make_workers_router(convex), dependencies=auth)
    # The wasm download is public (consumed by the website's Vercel build);
    # only the upload requires the service token.
    app.include_router(make_wasm_public_router())
    app.include_router(make_wasm_upload_router(), dependencies=auth)

    @app.post("/tasks/{task_id}/skill", dependencies=auth)
    async def put_skill(task_id: str, request: Request) -> dict[str, str]:
        """Store the skill text a run onboarded from and record its pointer.

        Args:
            task_id: The member task the skill snapshot belongs to.
            request: Request whose raw body is the combined skill markdown.

        Returns:
            A dict with the blob storage id under ``storageId``.
        """
        try:
            text = (await request.body()).decode()
        except UnicodeDecodeError as e:
            raise HTTPException(400, f"skill body must be UTF-8 text: {e}")
        storage_id = blobs.put_text("skills", task_id, text)
        await convex.mutation(
            "tasks:update", {"id": task_id, "patch": {"skillStorageId": storage_id}}
        )
        return {"storageId": storage_id}

    @app.get("/transcripts/{storage_id:path}", dependencies=auth)
    async def get_transcript(storage_id: str) -> Response:
        """Serve a stored transcript blob as plain text.

        Args:
            storage_id: Blob storage id (path) of the transcript.

        Returns:
            A text/plain Response containing the transcript body.

        Raises:
            HTTPException: 404 when no transcript exists for the id.
        """
        if not blobs.exists(storage_id):
            raise HTTPException(404, "transcript not found")
        return Response(content=blobs.get_text(storage_id), media_type="text/plain")

    return app


app = create_app()
