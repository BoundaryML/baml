"""Ingress gateway - the single public entry for all external events.

Every handler calls the service API (never Convex directly):
  /slack/events  -> create a task (source=slack)
  /notion/webhook -> flip an approved issue to status=approved (triggers fix)
  /bug           -> create a task (source=bug_report)
"""

from __future__ import annotations

import hashlib
import hmac
import logging
import os
import re
from collections import OrderedDict
from typing import Any, Optional

import httpx
from fastapi import BackgroundTasks, FastAPI, Header, HTTPException, Request, Response

from bench_core import slack_client
from bench_core.service_client import ServiceClient

# Use uvicorn's own logger so these lines actually reach stdout / Fly logs -
# uvicorn only configures its `uvicorn.*` loggers, so a bare getLogger("ingress")
# would be dropped by the last-resort handler.
log = logging.getLogger("uvicorn.error")

SERVICE_URL = os.environ["SERVICE_URL"]
SERVICE_TOKEN = os.environ.get("SERVICE_TOKEN", "")
SLACK_SIGNING_SECRET = os.environ.get("SLACK_SIGNING_SECRET", "")
# When set, /notion/webhook requires a valid X-Notion-Signature over the raw body
# (HMAC-SHA256 keyed by this token, the value shown in the Notion webhook UI).
# Left empty, signature verification is skipped (current default behavior).
NOTION_VERIFICATION_TOKEN = os.environ.get("NOTION_VERIFICATION_TOKEN", "")

app = FastAPI(title="baml-bench-ingress")
_service = ServiceClient(SERVICE_URL, SERVICE_TOKEN)
_seen: "OrderedDict[str, bool]" = OrderedDict()
_SEEN_CAP = 1024

_MENTION = re.compile(r"^\s*<@[^>]+>\s*")

# "@bammy babysit this PR https://github.com/o/r/pull/12" -> atb2 watches the PR
# until it merges (tools/atb2/baml_src/merge_issue.baml). The request is a row in
# the atb2 store; a runner with the toolchain serves it. Both settings are
# optional: without them a babysit mention is an ordinary task.
FEEDBACK_SUPABASE_URL = os.environ.get("FEEDBACK_SUPABASE_URL", "").rstrip("/")
FEEDBACK_SUPABASE_KEY = os.environ.get("FEEDBACK_SUPABASE_KEY", "")
SLACK_BOT_TOKEN = os.environ.get("SLACK_BOT_TOKEN", "")
_BABYSIT = re.compile(r"^\s*babysit\b", re.IGNORECASE)
# a GitHub PR url, bare or Slack-wrapped (<url> / <url|label>), trailing path or punctuation allowed
_PR_URL = re.compile(r"<?(https://github\.com/[\w.-]+/[\w.-]+/pull/\d+)")


def parse_babysit(text: str) -> Optional[str]:
    """The PR url of a babysit request, or None when ``text`` is not one.

    Args:
        text: The mention text with the leading bot mention already stripped.

    Returns:
        The canonical PR url (no trailing path, no Slack wrapping), or None.
    """
    if not _BABYSIT.search(text):
        return None
    m = _PR_URL.search(text)
    return m.group(1) if m else None


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


async def _create_slack_task(event: dict[str, Any], text: str, eid: Optional[str]) -> None:
    """Create a Slack-sourced task off the request path.

    Runs after we have already ACKed Slack, so a slow Convex write can never push
    us past Slack's 3s deadline. Failures are logged, not raised (the ACK is gone).

    Args:
        event: The Slack event object (channel, ts, thread_ts, user).
        text: The user's prompt, with a leading bot mention already stripped.
        eid: The Slack event id, for log correlation.
    """
    try:
        tid = await _service.create("tasks", {
            "source": "slack",
            "prompt": text,
            "slackChannel": event.get("channel"),
            "slackThreadTs": event.get("thread_ts") or event.get("ts"),
            "slackUser": event.get("user"),
            "status": "queued",
        })
        log.info("slack: created task %s event_id=%s text=%r", tid, eid, text[:80])
    except Exception:  # noqa: BLE001
        log.exception("slack: failed to create task for event_id=%s", eid)


async def _enqueue_babysit(event: dict[str, Any], pr: str, eid: Optional[str]) -> None:
    """Queue a babysit request in the atb2 store and ack in the thread.

    Off the request path, like ``_create_slack_task``. The row's ``event_id`` is
    unique, so a redelivered mention is one request (the second insert is a
    409 we ignore). Failures are logged, not raised.

    Args:
        event: The Slack event object (channel, ts, thread_ts, user).
        pr: The PR url to watch.
        eid: The Slack event id (dedup key in the store).
    """
    channel = event.get("channel") or ""
    thread_ts = event.get("thread_ts") or event.get("ts") or ""
    row = {
        "pr": pr,
        "channel": channel,
        "thread_ts": thread_ts,
        "requested_by": event.get("user"),
        "event_id": eid,
    }
    try:
        async with httpx.AsyncClient(timeout=10.0) as c:
            r = await c.post(
                f"{FEEDBACK_SUPABASE_URL}/rest/v1/babysit_requests",
                json=row,
                headers={
                    "apikey": FEEDBACK_SUPABASE_KEY,
                    "Authorization": f"Bearer {FEEDBACK_SUPABASE_KEY}",
                    "Prefer": "return=minimal",
                },
            )
        if r.status_code == 409:
            log.info("slack: babysit already queued event_id=%s", eid)
            return
        if r.status_code >= 300:
            log.error("slack: babysit insert failed %s event_id=%s body=%r", r.status_code, eid, r.text[:200])
            await slack_client.post_message(SLACK_BOT_TOKEN, channel,
                                            f":x: could not queue {pr} (store said {r.status_code})",
                                            thread_ts=thread_ts)
            return
        await slack_client.post_message(
            SLACK_BOT_TOKEN, channel,
            f":eyes: queued: a runner will watch {pr} until it merges and report here",
            thread_ts=thread_ts)
        log.info("slack: queued babysit event_id=%s pr=%s", eid, pr)
    except Exception:  # noqa: BLE001
        log.exception("slack: failed to queue babysit for event_id=%s", eid)


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
        pr = parse_babysit(text) if FEEDBACK_SUPABASE_URL and FEEDBACK_SUPABASE_KEY else None
        if pr:
            background_tasks.add_task(_enqueue_babysit, dict(event), pr, eid)
            log.info("slack: queued babysit event_id=%s pr=%s", eid, pr)
        elif text:
            background_tasks.add_task(_create_slack_task, dict(event), text, eid)
            log.info("slack: queued task create event_id=%s text=%r", eid, text[:80])
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
    if rows:
        await _service.update("issues", rows[0]["_id"], {"status": "approved"})
        log.info("notion approve -> issue %s approved (page=%s)", rows[0]["_id"], page_id)
    else:
        log.warning("notion webhook: no issue matches page %s", page_id)
    return Response(status_code=200)


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
    tid = await _service.create("tasks", {
        "source": "bug_report", "prompt": prompt,
        "notionProposerPageId": payload.get("notionPageId"), "status": "queued",
    })
    return {"id": tid}
