"""Unit coverage for the ingress routing logic (the bits most recently fixed):
Slack signature + mention-strip, the Notion page-id extraction precedence, and the
UUID-hyphen toggle. The ServiceClient is replaced with an in-memory fake - no backend,
no secrets.
"""

import hashlib
import hmac
import json
import time

import pytest
from fastapi.testclient import TestClient

from services.ingress import app as ing

DASHED = "11111111-2222-3333-4444-555555555555"
COMPACT = DASHED.replace("-", "")


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


# ---- pure helper ----

def test_toggle_uuid_hyphens_roundtrips():
    """_toggle_uuid_hyphens converts dashed <-> compact and passes non-UUIDs through."""
    assert ing._toggle_uuid_hyphens(DASHED) == COMPACT
    assert ing._toggle_uuid_hyphens(COMPACT) == DASHED
    assert ing._toggle_uuid_hyphens("not-a-uuid") == "not-a-uuid"


# ---- notion webhook page-id extraction ----

def test_notion_prefers_entity_id_over_event_id(fake_service):
    """The webhook looks up the page via entity.id, not the top-level event id.

    Args:
        fake_service: In-memory ServiceClient fake installed on the ingress module.
    """
    client = TestClient(ing.app)
    body = {"id": "evt-TOP-LEVEL-EVENT-ID", "type": "page.properties_updated",
            "entity": {"id": DASHED, "type": "page"}}
    r = client.post("/notion/webhook", json=body)
    assert r.status_code == 200
    # the lookup must use the page (entity.id), NOT the top-level event id
    assert fake_service.listed and fake_service.listed[0][1] == DASHED


def test_notion_verification_handshake_no_lookup(fake_service):
    """A verification_token handshake returns 200 without touching the issues table.

    Args:
        fake_service: In-memory ServiceClient fake installed on the ingress module.
    """
    client = TestClient(ing.app)
    r = client.post("/notion/webhook", json={"verification_token": "secret_abc"})
    assert r.status_code == 200
    assert fake_service.listed == []  # never hit the issues table


def test_notion_no_page_id_is_400(fake_service):
    """A webhook payload with no resolvable page id returns 400.

    Args:
        fake_service: In-memory ServiceClient fake installed on the ingress module.
    """
    client = TestClient(ing.app)
    r = client.post("/notion/webhook", json={"type": "ping"})
    assert r.status_code == 400


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
