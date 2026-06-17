"""Local-run ingest: turn a Claude Code `.jsonl` into a task + queued trophy.

A run done locally (no proxy) can be pushed into the pipeline here. We parse the
raw session transcript exactly as the proxy would (bench_core.transcript), create
a done task + a queued trophy, and let baml-dedup pick the trophy up so the full
task -> trophy -> dedup -> issues -> Linear flow runs over it.
"""

from __future__ import annotations

import os
from typing import Any, Optional

from fastapi import APIRouter
from pydantic import BaseModel

from bench_core.prices import prices_for
from bench_core.transcript import (
    compute_cost,
    parse_claude_session,
    parse_turn_log,
    render_terminal_transcript,
)

from ..convex_gateway import ConvexGateway
from .. import blobs

CLAUDE_MODEL = os.environ.get("CLAUDE_MODEL", "claude-sonnet-4-6")
UI_BASE_URL = os.environ.get("UI_BASE_URL", "https://new.boundaryml.com/atb")


class IngestBody(BaseModel):
    """Request body for ingesting a local Claude Code run."""

    prompt: str
    source: str = "local"
    bamlVersion: Optional[str] = None
    repo: Optional[str] = None
    ref: Optional[str] = None
    sha: Optional[str] = None
    model: Optional[str] = None
    transcript: str  # raw .jsonl session log
    trophyJson: Optional[dict[str, Any]] = None  # optional agent self-report


def _metrics(transcript: str, model: Optional[str]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Parse a transcript into a metrics bag and the structured turn log.

    Args:
        transcript: The raw .jsonl session log.
        model: Model id used to price the run (falls back to CLAUDE_MODEL).

    Returns:
        A (metrics, turn_log) tuple.
    """
    turn_log, api_calls = parse_turn_log(transcript)
    session = parse_claude_session(transcript)
    tool_calls = sum(len(t.get("tools") or []) for t in turn_log)
    prices = prices_for(model or CLAUDE_MODEL)
    cost = compute_cost(session, prices.model_dump()) if prices else None
    metrics = {
        "turns": session.get("turns") or len(turn_log),
        "tool_calls": session.get("tool_calls") or tool_calls,
        "api_calls": api_calls,
        "input_tokens": session.get("input_tokens"),
        "output_tokens": session.get("output_tokens"),
        "total_tokens": session.get("total_tokens"),
        "cache_read_tokens": session.get("cache_read_tokens"),
        "cache_write_tokens": session.get("cache_write_tokens"),
        "estimated_cost_usd": cost,
    }
    return metrics, turn_log


def make_ingest_router(convex: ConvexGateway) -> APIRouter:
    """Build the /ingest router.

    Args:
        convex: Gateway used to create the task + trophy rows.

    Returns:
        An APIRouter exposing ``POST /ingest/run``.
    """
    r = APIRouter(prefix="/ingest", tags=["ingest"])

    @r.post("/run")
    async def ingest_run(body: IngestBody) -> dict[str, str]:
        """Ingest a local run as a done task + queued trophy.

        Parses the transcript into metrics + a turn log, creates a ``done`` task
        and stores the transcript blob on it, then creates a ``queued`` trophy
        carrying the metrics, turn log, and any agent self-report so baml-dedup
        runs the full pipeline over it.

        Args:
            body: The ingest payload (prompt, transcript, optional trophyJson).

        Returns:
            A dict with the new ``taskId``, ``trophyId``, and dashboard ``runUrl``.
        """
        metrics, turn_log = _metrics(body.transcript, body.model)
        tj = body.trophyJson or {}
        files_created = tj.get("filesCreated") or tj.get("files_created") or {}
        if files_created:
            metrics["files_touched"] = len(files_created)
            metrics["loc_changed"] = sum(
                len((c or "").splitlines()) for c in files_created.values()
            )

        task_id = await convex.mutation("tasks:create", {"doc": {
            "source": body.source,
            "prompt": body.prompt,
            "repo": body.repo,
            "ref": body.ref,
            "sha": body.sha,
            "bamlVersion": body.bamlVersion,
            "status": "done",
        }})
        storage_id = blobs.put_text(
            "tasks", task_id, render_terminal_transcript(body.transcript)
        )
        await convex.mutation(
            "tasks:update", {"id": task_id, "patch": {"transcriptStorageId": storage_id}}
        )

        trophy_id = await convex.mutation("trophies:create", {"doc": {
            "taskId": task_id,
            "outcome": tj.get("outcome") or "success",
            "bamlVersion": body.bamlVersion,
            "metrics": metrics,
            "transcriptStorageId": storage_id,
            "turnLog": turn_log,
            "summary": tj.get("summary"),
            "whatWentWell": tj.get("what_went_well") or tj.get("whatWentWell"),
            "whatFailed": tj.get("what_failed") or tj.get("whatFailed"),
            "reportMd": tj.get("report_md") or tj.get("reportMd"),
            "findings": tj.get("findings"),
            "filesCreated": files_created or None,
            "suggestions": tj.get("suggestions"),
            "status": "queued",
        }})

        run_url = f"{UI_BASE_URL.rstrip('/')}/runs/{trophy_id}"
        return {"taskId": task_id, "trophyId": trophy_id, "runUrl": run_url}

    return r
