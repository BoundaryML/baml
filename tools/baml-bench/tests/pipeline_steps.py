"""Reusable per-hop pipeline steps + assertions shared by the integration tests
(``test_integration_hops.py``) and the system test (``test_system_pipeline.py``).

Each step takes a live ``ServiceClient`` (and, for the ingress hops, an httpx client
plus the ingress base URL) wired up by the ``bench_stack`` fixture. Service and
processor modules are imported lazily inside the functions so importing this module
during fast unit-test collection never drags in stack-only dependencies.

The ``bench_stack`` fixture is session-scoped, so all stack tests share one Convex
backend. Assertions therefore key off the caller's own task id (unique per call) or
match the canonical finding by ``kind``/``title`` rather than assuming an empty
database or a specific batch ordering in the dedup stub.
"""

from __future__ import annotations

import asyncio
import hashlib
import hmac
import json
import time
from typing import Any, Optional

# The bug prompt the worker stub is canned against (it always reports an enum-alias
# language finding at call_index 2); kept here so both tiers use the same wording.
WORKER_PROMPT = "implement an enum with an aliased value in baml"
LANGUAGE_ISSUE_TITLE_FRAGMENT = "enum alias"


async def create_bug_task(service, prompt: str = WORKER_PROMPT) -> str:
    """Create a queued ``bug_report`` task directly via the service.

    Args:
        service: The ServiceClient bound to the running api.
        prompt: The task prompt (defaults to the enum-alias bug the stub expects).

    Returns:
        The created task's id.
    """
    return await service.create("tasks", {
        "source": "bug_report", "prompt": prompt, "status": "queued",
    })


async def run_worker_assert_trophy(service, task_id: str) -> dict[str, Any]:
    """Drain BamlWorker once and assert the task completed with a well-formed trophy.

    Args:
        service: The ServiceClient bound to the running api.
        task_id: The id of the task this caller created (used to find its trophy).

    Returns:
        The trophy document created for ``task_id``.
    """
    from services.baml_worker.__main__ import BamlWorker

    await BamlWorker(service)._drain()
    t = await service.get("tasks", task_id)
    assert t["status"] == "done", f"expected task done, got {t['status']}"

    trophies = await service.list("trophies", field="status", value="queued",
                                  index="by_status_created")
    mine = [x for x in trophies if x["taskId"] == task_id]
    assert mine, "no trophy created for task"
    tr = mine[0]
    assert tr["outcome"] == "success", tr["outcome"]
    assert tr["metrics"]["api_calls"] == 2
    assert tr.get("reportMd"), "trophy should carry the agent's report_md"
    assert tr["findings"] and tr["findings"][0]["kind"] == "language"
    assert tr["findings"][0]["anchor"]["call_index"] == 2
    assert tr["findings"][0].get("suggestion"), "finding should carry a suggestion"
    assert tr.get("suggestions"), "trophy should carry top-level suggestions"
    return tr


async def run_dedup_assert_issue(service) -> dict[str, Any]:
    """Drain BamlDedup once and assert the canonical language issue exists.

    The dedup stub returns issues without an existing id, so each run inserts a fresh
    ``language``/enum-alias issue. Since ``bench_stack`` is session-scoped, several may
    accumulate across tests; this returns the newest (by Convex ``_creationTime``) so a
    caller operates on the issue its own dedup drain just created.

    Args:
        service: The ServiceClient bound to the running api.

    Returns:
        The most recently created deduplicated language issue document.
    """
    from services.baml_dedup.__main__ import BamlDedup

    await BamlDedup(service)._drain()
    issues = await service.list("issues", limit=100)
    lang = [i for i in issues if i.get("kind") == "language"
            and LANGUAGE_ISSUE_TITLE_FRAGMENT in (i.get("title") or "").lower()]
    assert lang, "dedup did not produce the expected language issue"
    iss = max(lang, key=lambda i: i.get("_creationTime") or i.get("firstSeenAt") or 0)
    assert iss.get("category") == "bug", iss.get("category")
    assert iss.get("suggestion"), "issue should carry a suggestion"
    assert any(e.get("call_index") == 2 for e in (iss.get("evidence") or [])), \
        "issue should carry evidence anchored at call_index 2"
    return iss


async def run_notion_push_assert_synced(service, issue_id: str) -> dict[str, Any]:
    """Drain NotionPush once (no-creds path) and assert the issue syncs + confirms.

    Args:
        service: The ServiceClient bound to the running api.
        issue_id: The issue to check after the push drain.

    Returns:
        The issue document after the push.
    """
    from services.notion_fixer.__main__ import NotionPush

    await NotionPush(service)._drain()
    iss = await service.get("issues", issue_id)
    assert iss["notionSyncStatus"] == "synced", iss["notionSyncStatus"]
    assert iss["status"] == "confirmed", iss["status"]
    return iss


def slack_headers(secret: str, body: bytes) -> dict[str, str]:
    """Build valid Slack v0 HMAC signature headers for a raw request body.

    Args:
        secret: The shared Slack signing secret.
        body: The exact request body bytes the signature is computed over.

    Returns:
        Headers with a current timestamp, a matching ``X-Slack-Signature``, and a
        JSON content type.
    """
    ts = str(int(time.time()))
    sig = "v0=" + hmac.new(secret.encode(), b"v0:" + ts.encode() + b":" + body,
                           hashlib.sha256).hexdigest()
    return {"X-Slack-Request-Timestamp": ts, "X-Slack-Signature": sig,
            "Content-Type": "application/json"}


async def post_bug_via_ingress(http, ingress_url: str, service,
                               prompt: str = WORKER_PROMPT) -> str:
    """POST ``/bug`` to ingress and assert a queued ``bug_report`` task is created.

    Args:
        http: An httpx.AsyncClient.
        ingress_url: The ingress base URL.
        service: The ServiceClient (used to read back the created task).
        prompt: The bug prompt to submit.

    Returns:
        The created task's id.
    """
    r = await http.post(f"{ingress_url}/bug", json={"prompt": prompt})
    assert r.status_code == 200, r.status_code
    task_id = r.json()["id"]
    t = await service.get("tasks", task_id)
    assert t["source"] == "bug_report" and t["status"] == "queued"
    return task_id


async def post_signed_slack_assert_task(http, ingress_url: str, secret: str, service) -> None:
    """Post a signed ``/slack/events`` app_mention and assert the stripped task lands.

    Also asserts a bad-signature request is rejected with 401.

    Args:
        http: An httpx.AsyncClient.
        ingress_url: The ingress base URL.
        secret: The shared Slack signing secret.
        service: The ServiceClient (used to read back the created slack task).
    """
    body = json.dumps({
        "type": "event_callback", "event_id": "Ev-steps-1",
        "event": {"type": "app_mention", "text": "<@U1> write a sorting function in baml",
                  "channel": "C1", "ts": "1.2", "user": "U9"},
    }).encode()
    r = await http.post(f"{ingress_url}/slack/events", content=body,
                        headers=slack_headers(secret, body))
    assert r.status_code == 200, r.status_code
    await asyncio.sleep(0.3)  # the create runs in a background task
    slack_tasks = await service.list("tasks", field="source", value="slack", index="by_source")
    assert any(x["prompt"] == "write a sorting function in baml" for x in slack_tasks)

    bad = await http.post(f"{ingress_url}/slack/events", content=body,
                          headers={"X-Slack-Request-Timestamp": str(int(time.time())),
                                   "X-Slack-Signature": "v0=bad"})
    assert bad.status_code == 401, bad.status_code


async def seed_synced_issue(service, *, kind: str = "skill", page_id: str = "pg-1") -> str:
    """Create a confirmed+synced issue carrying a Notion page id (for the approve path).

    Args:
        service: The ServiceClient bound to the running api.
        kind: The issue kind (``skill`` or ``language``).
        page_id: The Notion page id the webhook will look the issue up by.

    Returns:
        The created issue's id.
    """
    now = int(time.time() * 1000)
    return await service.create("issues", {
        "kind": kind, "title": "doc gap", "description": "x", "evidence": [],
        "status": "confirmed", "notionSyncStatus": "synced",
        "notionPageId": page_id, "firstSeenAt": now, "lastSeenAt": now,
    })


async def approve_issue_via_webhook(http, ingress_url: str, service, issue_id: str, *,
                                    page_id: str) -> None:
    """Approve an issue through ``/notion/webhook``, looked up by its Notion page id.

    Ensures the issue carries ``page_id`` (patching it when the no-creds push left
    none), posts the webhook, and asserts the issue flips to ``approved``.

    Args:
        http: An httpx.AsyncClient.
        ingress_url: The ingress base URL.
        service: The ServiceClient bound to the running api.
        issue_id: The issue to approve.
        page_id: The unique Notion page id used for the webhook lookup.
    """
    await service.update("issues", issue_id, {"notionPageId": page_id})
    r = await http.post(f"{ingress_url}/notion/webhook", json={"page_id": page_id})
    assert r.status_code == 200, r.status_code
    iss = await service.get("issues", issue_id)
    assert iss["status"] == "approved", iss["status"]


async def run_fixdispatch_assert_launch(service, issue_id: str,
                                        expected_ref: str = "agent-test-1") -> dict[str, Any]:
    """Drain FixDispatch once and assert the approved issue launches a Cursor agent.

    Args:
        service: The ServiceClient bound to the running api.
        issue_id: The approved issue to dispatch.
        expected_ref: The agent id the fake Cursor endpoint returns.

    Returns:
        The issue document after dispatch.
    """
    from services.notion_fixer.__main__ import FixDispatch

    await FixDispatch(service)._drain()
    iss = await service.get("issues", issue_id)
    assert iss["status"] == "fixing", iss["status"]
    assert iss.get("fixSlackTs") == expected_ref, iss.get("fixSlackTs")
    return iss
