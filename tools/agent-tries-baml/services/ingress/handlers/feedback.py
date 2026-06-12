"""Feedback ingestion: a free-text report becomes a done task + queued trophy.

Shared by the public POST /feedback endpoint (the `baml feedback` CLI) and the
@bammy feedback route. Mirrors the local-run ingest pattern
(services/api/routers/ingest.py): the queued trophy flows through dedup ->
issues -> Notion triage like any other run result, so feedback lands on the
same issue board with zero extra pipeline.

Two payload shapes from the CLI:
  * default       — just the issue text (+ min repro the reporter typed).
  * with context  — team members set BAML_FEEDBACK_INCLUDE_CONTEXT and the CLI
    also ships the Claude Code session transcript and the project's files;
    the transcript is stored as a blob (rendered + parsed into a turn log
    when it's a real session log) so the run page shows it like any agent run.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Optional

from bench_core.service_client import ServiceClient

log = logging.getLogger("uvicorn.error")


def _parse_transcript(raw: str) -> tuple[str, Optional[list], dict[str, Any]]:
    """Best-effort parse of a Claude Code .jsonl session transcript.

    Args:
        raw: The transcript text as uploaded.

    Returns:
        (blob_text, turn_log, metrics): the rendered terminal view (or the raw
        text when it isn't a parseable session), the structured turn log (or
        None), and any session metrics recovered (empty dict otherwise).
    """
    try:
        from bench_core.transcript import (
            parse_claude_session,
            parse_turn_log,
            render_terminal_transcript,
        )

        turn_log, api_calls = parse_turn_log(raw)
        if not turn_log:
            return raw, None, {}
        session = parse_claude_session(raw)
        metrics = {
            "turns": session.get("turns") or len(turn_log),
            "tool_calls": session.get("tool_calls"),
            "api_calls": api_calls,
            "input_tokens": session.get("input_tokens"),
            "output_tokens": session.get("output_tokens"),
            "total_tokens": session.get("total_tokens"),
        }
        return render_terminal_transcript(raw), turn_log, {k: v for k, v in metrics.items() if v is not None}
    except Exception:  # noqa: BLE001 — never let parsing sink the report
        log.exception("feedback: transcript parse failed; storing raw")
        return raw, None, {}


async def create_feedback(
    service: ServiceClient,
    message: str,
    *,
    baml_version: Optional[str] = None,
    os_name: Optional[str] = None,
    arch: Optional[str] = None,
    origin: str = "cli",
    slack: Optional[dict[str, Any]] = None,
    transcript: Optional[str] = None,
    files_created: Optional[dict[str, str]] = None,
) -> dict[str, str]:
    """Create the task + trophy pair for one piece of feedback.

    Args:
        service: Service client used for the creates.
        message: The feedback text (becomes the task prompt and trophy summary).
        baml_version: The reporter's baml version, when known.
        os_name: The reporter's OS, when known.
        arch: The reporter's CPU arch, when known.
        origin: Where the feedback came from ("cli" or "slack").
        slack: Optional slack routing (channel/thread/user) recorded on the task.
        transcript: Optional Claude Code session transcript (team opt-in via
            BAML_FEEDBACK_INCLUDE_CONTEXT); stored as a blob on the task/trophy.
        files_created: Optional {path: content} project files (same opt-in).

    Returns:
        A dict with the created ``taskId`` and ``trophyId``.
    """
    task_doc: dict[str, Any] = {
        "source": "feedback",
        "prompt": message,
        "bamlVersion": baml_version,
        "status": "done",
    }
    if slack:
        task_doc.update({k: v for k, v in slack.items() if v is not None})
    task_id = await service.create("tasks", task_doc)

    metrics: dict[str, Any] = {
        "origin": origin,
        "receivedAt": int(time.time() * 1000),
    }
    if baml_version:
        metrics["bamlVersion"] = baml_version
    if os_name:
        metrics["os"] = os_name
    if arch:
        metrics["arch"] = arch

    storage_id: Optional[str] = None
    turn_log = None
    if transcript:
        blob_text, turn_log, session_metrics = _parse_transcript(transcript)
        metrics.update(session_metrics)
        try:
            storage_id = await service.put_transcript("tasks", task_id, blob_text)
        except Exception:  # noqa: BLE001 — the report still lands without the blob
            log.exception("feedback: transcript blob upload failed")
            storage_id = None
    if files_created:
        metrics["files_touched"] = len(files_created)
        metrics["loc_changed"] = sum(
            len((c or "").splitlines()) for c in files_created.values()
        )

    trophy_id = await service.create("trophies", {
        "taskId": task_id,
        "source": "feedback",
        "outcome": "feedback",
        "bamlVersion": baml_version,
        "metrics": metrics,
        "summary": message,
        "transcriptStorageId": storage_id,
        "turnLog": turn_log,
        "filesCreated": files_created or None,
        "status": "queued",
    })
    return {"taskId": task_id, "trophyId": trophy_id}
