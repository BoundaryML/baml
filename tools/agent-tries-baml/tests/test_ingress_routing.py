"""Unit coverage for the ingress routing logic: Slack signature + mention-strip,
and the Linear webhook (signature verification, status-group label routing to
approved/redraft, and the bot-write loop guard). The ServiceClient is replaced
with an in-memory fake - no backend, no secrets.
"""

import hashlib
import hmac
import json
import time

import pytest
from fastapi.testclient import TestClient

from bench_core import linear_client as lc
from services.ingress import app as ing


class FakeService:
    """Records calls so tests can assert what the handler looked up / wrote."""

    def __init__(self, rows=None):
        """Initialize the fake with optional canned rows for ``list`` to return.

        Args:
            rows: Rows returned by every ``list`` call (defaults to empty).
        """
        self.rows = rows or []
        self.listed: list[tuple] = []
        self.updated: list[tuple] = []
        self.created: list[tuple] = []

    async def list(self, table, *, field=None, value=None, index=None, **kw):
        """Record a list query and return the canned rows.

        Args:
            table: The table name (ignored).
            field: The queried field; recorded for assertions.
            value: The queried value; recorded for assertions.
            index: The index name (ignored).
            **kw: Any extra query kwargs (ignored).

        Returns:
            The canned ``rows`` the fake was constructed with.
        """
        self.listed.append((field, value))
        return self.rows

    async def update(self, table, id, patch):
        """Record an update call.

        Args:
            table: The table name (ignored).
            id: The document id being patched; recorded for assertions.
            patch: The patch dict; recorded for assertions.

        Returns:
            An empty dict.
        """
        self.updated.append((id, patch))
        return {}

    async def create(self, table, doc):
        """Record a create call and return a fixed id.

        Args:
            table: The table name; recorded for assertions.
            doc: The document being created; recorded for assertions.

        Returns:
            The fixed id ``"task-1"``.
        """
        self.created.append((table, doc))
        return "task-1"


@pytest.fixture
def fake_service(monkeypatch):
    """Patch the ingress module's ServiceClient with an in-memory FakeService.

    Args:
        monkeypatch: pytest's monkeypatch fixture used to swap ``ing._service``.

    Returns:
        The FakeService instance now installed on the ingress module.
    """
    fake = FakeService()
    monkeypatch.setattr(ing, "_service", fake)
    return fake


def _slack_headers(body: bytes) -> dict:
    """Build valid Slack request headers (v0 HMAC signature) for a raw body.

    Args:
        body: The exact request body bytes the signature is computed over.

    Returns:
        Headers with a current timestamp, a matching ``X-Slack-Signature``, and a
        JSON content type.
    """
    ts = str(int(time.time()))
    sig = "v0=" + hmac.new(ing.SLACK_SIGNING_SECRET.encode(),
                           b"v0:" + ts.encode() + b":" + body, hashlib.sha256).hexdigest()
    return {"X-Slack-Request-Timestamp": ts, "X-Slack-Signature": sig,
            "Content-Type": "application/json"}


# ---- linear webhook routing ----

LINEAR_ID = "li_abc123"


def _issue_event(label_id: str, *, issue_id: str = LINEAR_ID, actor_id: str = "human-1") -> dict:
    """Build a Linear Issue webhook payload carrying a single status-group label."""
    return {"type": "Issue", "action": "update", "actor": {"id": actor_id},
            "data": {"id": issue_id, "labelIds": [label_id]}}


def test_linear_webhook_routes_approved(fake_service):
    """An approved label on a resting (confirmed) issue flips it to approved."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "confirmed"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_APPROVED))
    assert r.status_code == 200
    # looked up by linearIssueId, then flipped to approved
    assert fake_service.listed and fake_service.listed[0] == ("linearIssueId", LINEAR_ID)
    assert fake_service.updated == [("iss_1", {"status": "approved"})]


def test_linear_webhook_routes_redraft(fake_service):
    """A redraft label on a resting (confirmed) issue flips it to redraft."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "confirmed"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_REDRAFT))
    assert r.status_code == 200
    assert fake_service.updated == [("iss_1", {"status": "redraft"})]


def test_linear_webhook_bot_label_is_noop(fake_service):
    """A bot-written status label (e.g. to-cursor) routes to no-op, never a loop."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "tocursor"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_TO_CURSOR))
    assert r.status_code == 200
    assert fake_service.updated == []  # to-cursor is not a human trigger


def test_linear_webhook_approved_ignored_when_in_flight(fake_service):
    """The status-state gate: an approved label is ignored once the issue is past it.

    This is what makes the webhook safe against duplicate deliveries / echoed bot
    writes WITHOUT an actor check — so the bot may share a human's API token.
    """
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "tocursor"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_APPROVED))
    assert r.status_code == 200
    assert fake_service.updated == []  # already dispatched -> no re-dispatch


def test_linear_webhook_failed_reapproves(fake_service):
    """`failed` is a resting state: re-approving a dispatch that exhausted its
    attempts (e.g. a transient Cursor 400) is the human retry path, so it flips
    back to approved for a fresh dispatch."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "failed"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_APPROVED))
    assert r.status_code == 200
    assert fake_service.updated == [("iss_1", {"status": "approved"})]


def test_linear_webhook_prprep_restarts_fix_budget(fake_service):
    """Moving a needs_human issue back to pr-prep restarts its 3-attempt fix budget and
    clears the dispatch/dedup fields so the tracker re-tries the PR from scratch."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "needs_human",
                          "fixAttempts": 3, "lastFixedSha": "abc", "cursorAgentId": "bc-old"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_PR_PREP))
    assert r.status_code == 200
    assert fake_service.updated == [("iss_1", {"status": "prprep", "fixAttempts": 0,
                                               "lastFixedSha": "", "cursorAgentId": "",
                                               "fixSlackTs": ""})]


def test_linear_webhook_prprep_noop_when_not_needs_human(fake_service):
    """A pr-prep label on an issue already in prprep is the bot's own write — no-op
    (the status-state gate keeps the retry exclusive to needs_human)."""
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "prprep"}]
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_PR_PREP))
    assert r.status_code == 200
    assert fake_service.updated == []


def test_linear_webhook_no_issue_id_is_400(fake_service):
    """An Issue event with no data.id returns 400."""
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json={"type": "Issue", "data": {}})
    assert r.status_code == 400


def test_linear_webhook_unmatched_is_200(fake_service):
    """An issue we don't mirror (no linearIssueId match) is a 200 no-op."""
    fake_service.rows = []
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json=_issue_event(lc.LINEAR_STATUS_APPROVED))
    assert r.status_code == 200
    assert fake_service.updated == []


def test_linear_webhook_non_issue_type_is_200(fake_service):
    """A non-Issue event (e.g. Comment) is ignored with 200 and no lookup."""
    client = TestClient(ing.app)
    r = client.post("/linear/webhook", json={"type": "Comment", "data": {"id": "c1"}})
    assert r.status_code == 200
    assert fake_service.listed == []


def test_linear_webhook_bad_signature_401(fake_service, monkeypatch):
    """With a signing secret configured, a bad Linear-Signature is rejected 401."""
    monkeypatch.setattr(ing, "LINEAR_WEBHOOK_SECRET", "shh")
    client = TestClient(ing.app)
    body = json.dumps(_issue_event(lc.LINEAR_STATUS_APPROVED)).encode()
    r = client.post("/linear/webhook", content=body,
                    headers={"Linear-Signature": "deadbeef", "Content-Type": "application/json"})
    assert r.status_code == 401
    assert fake_service.updated == []


def test_linear_webhook_good_signature_routes(fake_service, monkeypatch):
    """A valid HMAC-SHA256 hex signature over the raw body is accepted and routed."""
    monkeypatch.setattr(ing, "LINEAR_WEBHOOK_SECRET", "shh")
    fake_service.rows = [{"_id": "iss_1", "linearIssueId": LINEAR_ID, "status": "confirmed"}]
    client = TestClient(ing.app)
    body = json.dumps(_issue_event(lc.LINEAR_STATUS_APPROVED)).encode()
    sig = hmac.new(b"shh", body, hashlib.sha256).hexdigest()
    r = client.post("/linear/webhook", content=body,
                    headers={"Linear-Signature": sig, "Content-Type": "application/json"})
    assert r.status_code == 200
    assert fake_service.updated == [("iss_1", {"status": "approved"})]


# ---- slack events ----

def test_slack_bad_signature_401(fake_service):
    """A bad Slack signature is rejected with 401 and creates no task.

    Args:
        fake_service: In-memory ServiceClient fake installed on the ingress module.
    """
    client = TestClient(ing.app)
    body = json.dumps({"event": {"type": "app_mention", "text": "<@U1> hi"}}).encode()
    r = client.post("/slack/events", content=body,
                    headers={"X-Slack-Request-Timestamp": str(int(time.time())),
                             "X-Slack-Signature": "v0=bad", "Content-Type": "application/json"})
    assert r.status_code == 401
    assert fake_service.created == []


def test_slack_mention_strips_and_creates_task(fake_service):
    """A signed app_mention strips the bot mention and creates a source=slack task.

    Args:
        fake_service: In-memory ServiceClient fake installed on the ingress module.
    """
    client = TestClient(ing.app)
    body = json.dumps({
        "type": "event_callback", "event_id": "Ev-unique-1",
        "event": {"type": "app_mention", "text": "<@U1> solve fizzbuzz in baml",
                  "channel": "C1", "ts": "1.2", "user": "U9"},
    }).encode()
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    # background create ran; the mention was stripped from the prompt
    assert fake_service.created, "expected a task to be created"
    table, doc = fake_service.created[0]
    assert table == "tasks"
    assert doc["prompt"] == "solve fizzbuzz in baml"
    assert doc["source"] == "slack"
