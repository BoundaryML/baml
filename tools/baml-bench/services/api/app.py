"""baml-bench API - the central HTTP API and sole Convex gateway.

Exposes uniform CRUD + queue verbs + SSE per table (tasks, trophies,
issues, bamlBuilds), the baml version endpoints, and transcript blobs.
Bearer-token auth on every route.
"""

from __future__ import annotations

import hmac
import os

from fastapi import Depends, FastAPI, Header, HTTPException, Response

from .convex_gateway import gateway_from_env
from . import blobs
from .routers.table import make_router
from .routers.baml_builds import make_baml_router

SERVICE_TOKEN = os.environ.get("SERVICE_TOKEN", "")
TABLES = ["tasks", "trophies", "issues", "bamlBuilds"]


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
    app = FastAPI(title="baml-bench-api")
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
