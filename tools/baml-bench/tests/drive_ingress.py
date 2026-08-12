"""Integration driver for ingress + fix dispatch. Expects a running service
(SERVICE_URL) and ingress (INGRESS_URL); SLACK_SIGNING_SECRET shared."""

import asyncio
import hashlib
import hmac
import json
import os
import time

import httpx

from bench_core.service_client import ServiceClient
from services.notion_fixer.__main__ import FixDispatch

INGRESS = os.environ["INGRESS_URL"]
SECRET = os.environ["SLACK_SIGNING_SECRET"]


def slack_headers(body: bytes) -> dict:
    """Build valid Slack request headers (v0 HMAC signature) for a raw body.

    Args:
        body: The exact request body bytes the signature is computed over.

    Returns:
        Headers with a current timestamp, a matching ``X-Slack-Signature``, and a
        JSON content type.
    """
    ts = str(int(time.time()))
    sig = "v0=" + hmac.new(SECRET.encode(), b"v0:" + ts.encode() + b":" + body,
                           hashlib.sha256).hexdigest()
    return {"X-Slack-Request-Timestamp": ts, "X-Slack-Signature": sig,
            "Content-Type": "application/json"}


async def main() -> None:
    """Drive the ingress endpoints + fix dispatch end-to-end and assert each step.

    Exercises ``/bug`` (creates a bug_report task), a signed ``/slack/events``
    app_mention (mention stripped, slack task created), a bad-signature rejection,
    ``/notion/webhook`` (approves an issue by page id), and then FixDispatch
    (claims the approved issue to fixing and launches a Cursor cloud agent against
    the fake_proxy stub).

    Raises:
        AssertionError: If any endpoint or dispatch step produces an unexpected result.
    """
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("SERVICE_TOKEN", ""))
    async with httpx.AsyncClient(timeout=20.0) as http:
        # /bug
        r = await http.post(f"{INGRESS}/bug", json={"prompt": "fix the parser"})
        bug_id = r.json()["id"]
        t = await service.get("tasks", bug_id)
        assert t["source"] == "bug_report" and t["status"] == "queued"
        print("ingress /bug OK ->", bug_id)

        # /slack/events with valid signature
        body = json.dumps({
            "type": "event_callback", "event_id": "Ev123",
            "event": {"type": "app_mention", "text": "<@U1> write a sorting function in baml",
                      "channel": "C1", "ts": "1.2", "user": "U9"},
        }).encode()
        r = await http.post(f"{INGRESS}/slack/events", content=body, headers=slack_headers(body))
        assert r.status_code == 200, r.status_code
        await asyncio.sleep(0.3)
        slack_tasks = await service.list("tasks", field="source", value="slack", index="by_source")
        assert any(x["prompt"] == "write a sorting function in baml" for x in slack_tasks)
        print("ingress /slack/events OK (signature verified, mention stripped)")

        # bad signature rejected
        r = await http.post(f"{INGRESS}/slack/events", content=body,
                            headers={"X-Slack-Request-Timestamp": str(int(time.time())),
                                     "X-Slack-Signature": "v0=bad"})
        assert r.status_code == 401
        print("ingress /slack bad-signature -> 401 OK")

        # /notion/webhook -> approve an issue
        now = int(time.time() * 1000)
        iid = await service.create("issues", {
            "kind": "skill", "title": "doc gap", "description": "x", "evidence": [],
            "status": "confirmed", "notionSyncStatus": "synced",
            "notionPageId": "pg-1", "firstSeenAt": now, "lastSeenAt": now,
        })
        r = await http.post(f"{INGRESS}/notion/webhook", json={"page_id": "pg-1"})
        assert r.status_code == 200
        iss = await service.get("issues", iid)
        assert iss["status"] == "approved", iss["status"]
        print("ingress /notion/webhook OK -> issue approved")

        # FixDispatch claims approved -> fixing and launches a Cursor cloud agent
        # (CURSOR_API_BASE points at fake_proxy's /v1/agents stub).
        fd = FixDispatch(service)
        await fd._drain()
        iss = await service.get("issues", iid)
        assert iss["status"] == "fixing", iss["status"]
        assert iss.get("fixSlackTs") == "agent-test-1", iss.get("fixSlackTs")
        print("fix-dispatch OK -> issue fixing, cursor agent launched (repo=baml-skill for kind=skill)")
        print("INGRESS_OK")
    await service.aclose()


if __name__ == "__main__":
    asyncio.run(main())
