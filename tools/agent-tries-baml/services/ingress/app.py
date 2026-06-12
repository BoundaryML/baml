"""Ingress gateway - the single public entry for all external events.

Every handler calls the service API (never Convex directly):
  /slack/events  -> the @bammy router (bench tasks, changelog edits, promo
                    codes, feedback — see services/ingress/bammy.py)
  /notion/webhook -> read the page status and route the issue to approved (fix)
                     or redraft (human-feedback redraft loop)
  /bug           -> create a task (source=bug_report)
  /entries       -> public changelog JSON (consumed by the website build)
  /feedback      -> public, rate-limited feedback intake (`baml feedback` CLI)
"""

from __future__ import annotations

import hashlib
import hmac
import logging
import os
import re
import time
from collections import OrderedDict
from typing import Any, Optional

from fastapi import BackgroundTasks, FastAPI, Header, HTTPException, Request, Response

from bench_core import slack_client
from bench_core.notion_client import NotionClient
from bench_core.service_client import ServiceClient

from . import bammy
from .handlers import feedback as feedback_handler
from .handlers.bench import _create_cohort  # noqa: F401  (re-exported for tests/compat)
from .handlers.bench import _parse_branches, _parse_run_directives

# Use uvicorn's own logger so these lines actually reach stdout / Fly logs -
# uvicorn only configures its `uvicorn.*` loggers, so a bare getLogger("ingress")
# would be dropped by the last-resort handler.
log = logging.getLogger("uvicorn.error")

SERVICE_URL = os.environ["SERVICE_URL"]
SERVICE_TOKEN = os.environ.get("ATB_SERVICE_TOKEN", "")
SLACK_SIGNING_SECRET = os.environ.get("ATB_SLACK_SIGNING_SECRET", "")
# When set, /notion/webhook requires a valid X-Notion-Signature over the raw body
# (HMAC-SHA256 keyed by this token, the value shown in the Notion webhook UI).
# Left empty, signature verification is skipped (current default behavior).
NOTION_VERIFICATION_TOKEN = os.environ.get("NOTION_VERIFICATION_TOKEN", "")
# Integration token used to READ a page's status when routing a webhook (approve
# vs redraft). Without it, the webhook falls back to the legacy approve-only path.
NOTION_TOKEN = os.environ.get("ATB_NOTION_TOKEN", "")
# Notion board status labels the webhook routes on (must match notion_fixer's).
# Matched case-insensitively, so "Approved" / "approved" both register.
NOTION_STATUS_APPROVED = os.environ.get("NOTION_STATUS_APPROVED", "approved")
NOTION_STATUS_REDRAFT = os.environ.get("NOTION_STATUS_REDRAFT", "redraft")

app = FastAPI(title="agent-tries-baml-ingress")
_service = ServiceClient(SERVICE_URL, SERVICE_TOKEN)
_notion = NotionClient(NOTION_TOKEN) if NOTION_TOKEN else None
_seen: "OrderedDict[str, bool]" = OrderedDict()
_SEEN_CAP = 1024

_MENTION = re.compile(r"^\s*<@[^>]+>\s*")

# --- /feedback rate limiting (in-process; fine for a single Fly machine) ---
# The endpoint is public (a token shipped in a public CLI is not a secret), so
# spend is bounded instead: a small per-IP token bucket plus a global daily cap.
_FEEDBACK_BUCKET_CAP = float(os.environ.get("FEEDBACK_BURST", "3"))
_FEEDBACK_PER_HOUR = float(os.environ.get("FEEDBACK_PER_HOUR", "5"))
_FEEDBACK_DAILY_CAP = int(os.environ.get("FEEDBACK_DAILY_CAP", "100"))
# Body cap covers the team's opt-in context payload (transcript + files);
# the message itself stays capped at 8000 chars, and the context pieces get
# their own caps below so one report can't blow the blob volume.
_FEEDBACK_MAX_BYTES = 4 * 1024 * 1024
_FEEDBACK_MAX_TRANSCRIPT_CHARS = 2 * 1024 * 1024
_FEEDBACK_MAX_FILE_CHARS = 256 * 1024
_FEEDBACK_MAX_TOTAL_FILE_CHARS = 1024 * 1024
_fb_buckets: dict[str, tuple[float, float]] = {}  # ip -> (tokens, last_refill)
_fb_day: list[Any] = [0, 0]  # [yyyymmdd, count]


def _feedback_allowed(ip: str) -> bool:
    """Consume one feedback token for an IP; False when rate-limited.

    Args:
        ip: Client IP (Fly-Client-IP header, or the socket peer).

    Returns:
        True when the request may proceed.
    """
    now = time.time()
    today = int(time.strftime("%Y%m%d"))
    if _fb_day[0] != today:
        _fb_day[0], _fb_day[1] = today, 0
    if _fb_day[1] >= _FEEDBACK_DAILY_CAP:
        return False
    tokens, last = _fb_buckets.get(ip, (_FEEDBACK_BUCKET_CAP, now))
    tokens = min(_FEEDBACK_BUCKET_CAP, tokens + (now - last) * _FEEDBACK_PER_HOUR / 3600.0)
    if tokens < 1.0:
        _fb_buckets[ip] = (tokens, now)
        return False
    _fb_buckets[ip] = (tokens - 1.0, now)
    _fb_day[1] += 1
    if len(_fb_buckets) > 4096:  # bound memory under address churn
        _fb_buckets.clear()
    return True


def _is_duplicate(event_id: Optional[str]) -> bool:
    """Record an event id and report whether it was already seen.

    Backed by a bounded LRU (``_SEEN_CAP`` entries) so retried deliveries of the
    same event collapse to no-ops without growing memory unbounded.

    Args:
        event_id: The provider's delivery/event id, or None when absent.

    Returns:
        True if this event id was already handled (a duplicate); False on first
        sight (and the id is then recorded). Always False for a missing id.
    """
    if not event_id:
        return False
    if event_id in _seen:
        return True
    _seen[event_id] = True
    if len(_seen) > _SEEN_CAP:
        _seen.popitem(last=False)
    return False


@app.get("/healthz")
async def healthz() -> str:
    """Liveness probe.

    Returns:
        The literal string "ok".
    """
    return "ok"


async def _route_mention(event: dict[str, Any], text: str, eid: Optional[str]) -> None:
    """Route an app_mention through @bammy, reading _service at call time.

    A thin indirection so tests that monkeypatch ``ing._service`` keep
    working — the live (or faked) client is resolved when the background
    task runs, not when the module is imported.

    Args:
        event: The Slack event object.
        text: The mention text with the leading bot mention stripped.
        eid: The Slack event id, for log correlation.
    """
    await bammy.route_mention(_service, event, text, eid)


@app.post("/slack/events")
async def slack_events(request: Request,
                       background_tasks: BackgroundTasks,
                       x_slack_signature: str = Header(default=""),
                       x_slack_request_timestamp: str = Header(default=""),
                       x_slack_retry_num: str = Header(default="")) -> Any:
    """Handle the Slack Events API callback (URL verification + app mentions).

    ACKs instantly and creates any task in the background, so retries are rare;
    when they do arrive they carry the same event_id and dedup as no-ops. A bad
    signature is rejected with 401.

    Args:
        request: The raw inbound request (body is read for signature checking).
        background_tasks: FastAPI background runner for the deferred task create.
        x_slack_signature: Slack's ``X-Slack-Signature`` header.
        x_slack_request_timestamp: Slack's ``X-Slack-Request-Timestamp`` header.
        x_slack_retry_num: Slack's ``X-Slack-Retry-Num`` header (delivery attempt).

    Returns:
        The challenge dict for a url_verification handshake, or a Response: 401 on
        a bad signature, 200 otherwise (handled, deduped, or ignored).
    """
    raw = await request.body()
    import json
    body = json.loads(raw or b"{}")

    # URL verification handshake
    if body.get("type") == "url_verification":
        return {"challenge": body.get("challenge")}

    if not slack_client.verify_signature(
        SLACK_SIGNING_SECRET, x_slack_request_timestamp, raw, x_slack_signature
    ):
        log.warning("slack: bad signature (ts=%s)", x_slack_request_timestamp)
        return Response(status_code=401)

    event = body.get("event") or {}
    eid = body.get("event_id")
    etype = event.get("type")

    # We ACK instantly and do the create in the background, so retries should be
    # rare; when they do arrive they carry the same event_id and dedup as no-ops.
    if _is_duplicate(eid):
        log.info("slack: deduped event_id=%s type=%s retry#=%s", eid, etype, x_slack_retry_num or "0")
        return Response(status_code=200)

    if etype == "app_mention":
        text = _MENTION.sub("", event.get("text", "")).strip()
        if text:
            background_tasks.add_task(_route_mention, dict(event), text, eid)
            log.info("slack: queued bammy route event_id=%s text=%r", eid, text[:80])
        else:
            log.warning("slack: app_mention with empty text after strip; raw=%r event_id=%s",
                        (event.get("text") or "")[:120], eid)
        return Response(status_code=200)

    log.info("slack: ignored event type=%s event_id=%s", etype, eid)
    return Response(status_code=200)


@app.post("/notion/webhook")
async def notion_webhook(request: Request,
                         x_notion_signature: str = Header(default="")) -> Response:
    """Approve the issue a Notion webhook points at (the fix dispatcher claims it).

    Notion fires when an issue's Status becomes 'approved'. We find the issue by
    page id and flip it to approved.

    When ``NOTION_VERIFICATION_TOKEN`` is configured, the request's
    ``X-Notion-Signature`` (``sha256=<hmac>`` over the raw body) is verified first
    and a mismatch is rejected with 401.

    Notion's integration webhooks put the *page* id under ``entity.id`` (or
    ``data.id`` for automation webhooks); the top-level ``id`` is the
    *event/delivery* id, NOT the page - so it must be tried last, never first.
    Notion is also inconsistent about hyphens in ids, so the alternate
    hyphenation is retried before giving up.

    Args:
        request: The inbound webhook request (raw body read for verification).
        x_notion_signature: Notion's ``X-Notion-Signature`` header.

    Returns:
        A Response: 200 for the verification handshake or a handled/unmatched
        event, 401 on a bad signature, 400 when no page id can be found.
    """
    import json
    raw = await request.body()
    body = json.loads(raw or b"{}")

    # Subscription verification handshake: Notion sends this once when the webhook
    # is created and expects a 200. Do not log the token value (it is shown in the
    # Notion UI; copy it into NOTION_VERIFICATION_TOKEN from there).
    if "verification_token" in body:
        log.info("notion webhook: received subscription verification request")
        return Response(status_code=200)

    # Optional signature verification (no-op unless a token is configured).
    if NOTION_VERIFICATION_TOKEN:
        expected = "sha256=" + hmac.new(
            NOTION_VERIFICATION_TOKEN.encode(), raw, hashlib.sha256
        ).hexdigest()
        if not hmac.compare_digest(x_notion_signature, expected):
            log.warning("notion webhook: bad signature")
            return Response(status_code=401)

    page_id = (
        body.get("page_id")                       # synthetic / explicit
        or (body.get("entity") or {}).get("id")   # integration webhook: the page
        or (body.get("data") or {}).get("id")     # automation webhook: the page
        or body.get("id")                          # last resort (often the event id)
    )
    if not page_id:
        log.warning("notion webhook: no page id in payload keys=%s", sorted(body.keys()))
        return Response(status_code=400)

    rows = await _service.list("issues", field="notionPageId", value=page_id,
                               index="by_notion_page")
    if not rows:
        # Notion APIs are inconsistent about hyphens in ids; retry the
        # alternate form before giving up.
        alt = _toggle_uuid_hyphens(page_id)
        if alt != page_id:
            rows = await _service.list("issues", field="notionPageId", value=alt,
                                       index="by_notion_page")
    if not rows:
        log.warning("notion webhook: no issue matches page %s", page_id)
        return Response(status_code=200)

    issue = rows[0]
    new_status = await _route_notion_status(issue, page_id)
    if new_status:
        await _service.update("issues", issue["_id"], {"status": new_status})
        log.info("notion webhook -> issue %s status=%s (page=%s)", issue["_id"], new_status, page_id)
    return Response(status_code=200)


async def _route_notion_status(issue: dict[str, Any], page_id: str) -> Optional[str]:
    """Decide the issue status a Notion webhook should set.

    Reads the page's current Status (when a Notion token is configured) and maps
    the approved/redraft labels to their issue lifecycle states. Without a token
    it preserves the legacy approve-only behavior.

    Args:
        issue: The matched issue row (its stored notionPageId is preferred for
            the status read).
        page_id: The page id resolved from the webhook payload (fallback).

    Returns:
        The target issue status ("approved" or "redraft"), or None to do nothing.
    """
    if not _notion:
        return "approved"  # legacy: no Notion read configured, approve as before
    try:
        page_status = await _notion.get_status(issue.get("notionPageId") or page_id)
    except Exception:  # noqa: BLE001
        log.exception("notion webhook: failed to read status for page %s", page_id)
        return None
    # Case-insensitive match so "Approved" / "approved" both register.
    norm = (page_status or "").strip().lower()
    if norm == NOTION_STATUS_APPROVED.strip().lower():
        return "approved"
    if norm == NOTION_STATUS_REDRAFT.strip().lower():
        return "redraft"
    log.info("notion webhook: page %s status=%r -> no action", page_id, page_status)
    return None


def _toggle_uuid_hyphens(value: str) -> str:
    """Return the alternate hyphenation of a Notion id.

    Args:
        value: A Notion id in either 32-char compact or dashed-UUID form.

    Returns:
        The other form (compact <-> dashed). The value unchanged if it is not a
        32-character id.
    """
    compact = value.replace("-", "")
    if len(compact) != 32:
        return value
    if "-" in value:
        return compact
    return f"{compact[:8]}-{compact[8:12]}-{compact[12:16]}-{compact[16:20]}-{compact[20:]}"


@app.post("/bug")
async def bug_trigger(payload: dict) -> dict[str, str]:
    """Create a task from a bug report.

    Args:
        payload: The request body. Requires a non-empty ``prompt``; optional
            ``notionPageId`` (recorded as the proposer's Notion page).

    Returns:
        A dict ``{"id": <task id>}`` for the created task.

    Raises:
        HTTPException: 400 when ``prompt`` is missing or empty.
    """
    prompt = (payload.get("prompt") or "").strip() if isinstance(payload, dict) else ""
    if not prompt:
        raise HTTPException(status_code=400, detail="prompt required")
    # Honor inline [baml=N]/[canary]/[coldstart] directives, plus explicit
    # payload overrides for each field.
    prompt, opts = _parse_run_directives(prompt)
    if payload.get("bamlChannel"):
        opts["bamlChannel"] = str(payload["bamlChannel"]).lower()
    if payload.get("coldStart"):
        opts.pop("bamlPin", None)
        opts["coldStart"] = True
    elif payload.get("bamlPin"):
        opts["bamlPin"] = str(payload["bamlPin"])
    # A skill-arena bug fans out a cohort. Branches come from the inline directive
    # or an explicit payload `arenaBranches` (string or list).
    branches = opts.pop("arenaBranches", None)
    if payload.get("arenaBranches"):
        raw = payload["arenaBranches"]
        branches = _parse_branches(",".join(raw) if isinstance(raw, list) else str(raw))
    if branches:
        cid = await _create_cohort(_service, prompt, branches, opts, source="bug_report")
        return {"cohortId": cid}
    tid = await _service.create("tasks", {
        "source": "bug_report", "prompt": prompt,
        "notionProposerPageId": payload.get("notionPageId"), "status": "queued",
        **opts,
    })
    return {"id": tid}


# --------------------------------------------------------------------------- #
# Changelog (public read for the website + token-gated ops writes)
# --------------------------------------------------------------------------- #

_ENTRY_FIELDS = ("version", "date", "title", "body", "authors", "channel")


@app.get("/entries")
async def list_entries() -> dict[str, list[dict[str, Any]]]:
    """Serve the published changelog as JSON (the website build's contract).

    Shape matches the old baml-changelog2 service: ``{"entries": [...]}`` with
    one object per published (status=done) entry, newest-first by date.

    Returns:
        The published entries with version/date/title/body/authors/channel.
    """
    rows = await _service.list(
        "changelogEntries", field="status", value="done",
        index="by_status_created", limit=1000,
    )
    rows.sort(key=lambda r: (r.get("date") or "", r.get("createdAt") or 0), reverse=True)
    entries = [
        {k: e.get(k) for k in _ENTRY_FIELDS}
        for e in rows
    ]
    return {"entries": entries}


def _require_service_token(authorization: str) -> None:
    """Enforce the shared service token on ops endpoints.

    Args:
        authorization: The request's Authorization header value.

    Raises:
        HTTPException: 401 when a token is configured and does not match.
    """
    if not SERVICE_TOKEN:
        return  # dev mode
    if not hmac.compare_digest(authorization or "", f"Bearer {SERVICE_TOKEN}"):
        raise HTTPException(401, "unauthorized")


@app.post("/entries/update")
async def update_entries(authorization: str = Header(default="")) -> dict[str, Any]:
    """Sync the changelog now: enqueue entries for any missed releases.

    The same sweep the cron poller runs every CHANGELOG_POLL_SECS — exposed
    on demand for when the cron missed a release or was down. Idempotent:
    releases that already have an entry row (any status) are skipped.

    Args:
        authorization: Bearer token, checked against ATB_SERVICE_TOKEN.

    Returns:
        ``{"enqueued": [{version, tag, channel, id}, ...]}`` — empty when the
        changelog is already up to date.

    Raises:
        HTTPException: 401 on auth, 502 when the GitHub listing fails.
    """
    _require_service_token(authorization)
    from bench_core import changelog_github
    from bench_core.changelog_sync import sync_missing_entries

    try:
        enqueued = await sync_missing_entries(_service)
    except changelog_github.GitHubError as e:
        raise HTTPException(502, f"github: {e}")
    log.info("entries/update: enqueued %d release(s)", len(enqueued))
    return {"enqueued": enqueued}


@app.post("/entries")
async def create_entry(payload: dict,
                       authorization: str = Header(default="")) -> dict[str, str]:
    """Enqueue changelog generation for a release tag (ops parity endpoint).

    Args:
        payload: Requires ``release`` (a GitHub tag); optional ``from_release``
            overrides the predecessor used for the diff.
        authorization: Bearer token, checked against ATB_SERVICE_TOKEN.

    Returns:
        A dict with the entry row id under ``id``.

    Raises:
        HTTPException: 400 on a missing/unrecognized release tag, 401 on auth,
            409 when generation is already in flight for the version.
    """
    _require_service_token(authorization)
    from bench_core import changelog_github

    tag = (payload.get("release") or "").strip()
    if not tag:
        raise HTTPException(400, "release required")
    channel = changelog_github.channel_of(tag)
    if channel is None:
        raise HTTPException(400, f"unrecognized release tag: {tag}")
    version = changelog_github.normalize(tag)

    existing = await _service.list(
        "changelogEntries", field="version", value=version, index="by_version", limit=1
    )
    if existing:
        row = existing[0]
        if row.get("status") in ("queued", "generating"):
            raise HTTPException(409, f"generation already in flight for {version}")
        await _service.update("changelogEntries", row["_id"], {
            "tag": tag, "fromRelease": payload.get("from_release"),
            "reviseMode": "regenerate", "reviseGuidance": None,
        })
        await _service.transition("changelogEntries", row["_id"], "queued")
        return {"id": row["_id"]}
    eid = await _service.create("changelogEntries", {
        "version": version, "tag": tag, "channel": channel,
        "fromRelease": payload.get("from_release"), "status": "queued",
    })
    return {"id": eid}


@app.post("/entries/{version}/revise")
async def revise_entry(version: str, payload: dict,
                       authorization: str = Header(default="")) -> dict[str, str]:
    """Requeue an existing entry with revise guidance (ops parity endpoint).

    Args:
        version: The stored (normalized) version to revise.
        payload: Requires ``guidance`` free text.
        authorization: Bearer token, checked against ATB_SERVICE_TOKEN.

    Returns:
        A dict with the entry row id under ``id``.

    Raises:
        HTTPException: 400 on missing guidance, 401 on auth, 404 when the
            version has no entry, 409 when an edit is already in flight.
    """
    _require_service_token(authorization)
    guidance = (payload.get("guidance") or "").strip()
    if not guidance:
        raise HTTPException(400, "guidance required")
    rows = await _service.list(
        "changelogEntries", field="version", value=version, index="by_version", limit=1
    )
    if not rows:
        raise HTTPException(404, f"no entry for {version}")
    row = rows[0]
    if row.get("status") in ("queued", "generating"):
        raise HTTPException(409, f"generation already in flight for {version}")
    await _service.update("changelogEntries", row["_id"], {
        "reviseMode": "revise", "reviseGuidance": guidance,
    })
    await _service.transition("changelogEntries", row["_id"], "queued")
    return {"id": row["_id"]}


# --------------------------------------------------------------------------- #
# Feedback (public, rate-limited; the `baml feedback` CLI posts here)
# --------------------------------------------------------------------------- #


@app.post("/feedback")
async def feedback(request: Request) -> dict[str, str]:
    """Accept one piece of free-text feedback and queue it into the pipeline.

    Creates a done task + queued trophy (source=feedback) so dedup triages it
    onto the issue board. Public but rate-limited per client IP and per day.

    Args:
        request: The inbound request; JSON body with ``message`` (1..8000
            chars) plus optional ``bamlVersion``/``os``/``arch``/``source``.

    Returns:
        A dict with the created ``taskId`` and ``trophyId``.

    Raises:
        HTTPException: 400 on a missing/oversized message or unparsable body,
            413 on an oversized body, 429 when rate-limited.
    """
    raw = await request.body()
    if len(raw) > _FEEDBACK_MAX_BYTES:
        raise HTTPException(413, "feedback too large")
    ip = request.headers.get("Fly-Client-IP") or (request.client.host if request.client else "?")
    if not _feedback_allowed(ip):
        raise HTTPException(429, "rate limited; try again later")
    import json
    try:
        payload = json.loads(raw or b"{}")
    except json.JSONDecodeError:
        raise HTTPException(400, "invalid JSON body")
    message = (payload.get("message") or "").strip() if isinstance(payload, dict) else ""
    if not message or len(message) > 8000:
        raise HTTPException(400, "message required (1..8000 chars)")

    # Optional context (team opt-in via BAML_FEEDBACK_INCLUDE_CONTEXT in the
    # CLI): the session transcript and the project's files. Oversized pieces
    # are trimmed, never rejected — a fat transcript shouldn't lose the report.
    transcript = payload.get("transcript")
    if transcript is not None and not isinstance(transcript, str):
        raise HTTPException(400, "transcript must be a string")
    if transcript and len(transcript) > _FEEDBACK_MAX_TRANSCRIPT_CHARS:
        # Keep the tail: the end of a session is where the problem usually is.
        transcript = transcript[-_FEEDBACK_MAX_TRANSCRIPT_CHARS:]
    files_created = payload.get("filesCreated")
    if files_created is not None:
        if not isinstance(files_created, dict) or not all(
            isinstance(k, str) and isinstance(v, str) for k, v in files_created.items()
        ):
            raise HTTPException(400, "filesCreated must be a {path: content} object")
        trimmed: dict[str, str] = {}
        total = 0
        for path, content in files_created.items():
            content = content[:_FEEDBACK_MAX_FILE_CHARS]
            if total + len(content) > _FEEDBACK_MAX_TOTAL_FILE_CHARS:
                break
            trimmed[path] = content
            total += len(content)
        files_created = trimmed or None

    ids = await feedback_handler.create_feedback(
        _service, message,
        baml_version=payload.get("bamlVersion"),
        os_name=payload.get("os"),
        arch=payload.get("arch"),
        origin=str(payload.get("source") or "cli"),
        transcript=transcript or None,
        files_created=files_created,
    )
    log.info("feedback: accepted from %s -> trophy %s (transcript=%s files=%d)",
             ip, ids["trophyId"], bool(transcript), len(files_created or {}))
    return ids
