"""Unit coverage for the monolith absorption: the @bammy router (directive
fast-path + stubbed classifier dispatch), the public /entries and /feedback
ingress endpoints, and the new api surfaces (promo claim, worker presence,
changelogEntries queue) against the in-process MemoryGateway. No backend, no
secrets, no model calls.
"""

import hashlib
import hmac
import json
import time

import pytest
from fastapi.testclient import TestClient

from services.ingress import app as ing
from services.ingress import bammy


# ---------------------------------------------------------------------------
# fakes / helpers
# ---------------------------------------------------------------------------

class FakeService:
    """Records calls so tests can assert what the handlers looked up / wrote."""

    def __init__(self, rows=None):
        """Initialize the fake with optional canned rows for ``list``.

        Args:
            rows: Rows returned by every ``list`` call (defaults to empty).
        """
        self.rows = rows or []
        self.listed: list = []
        self.updated: list = []
        self.created: list = []
        self.transitioned: list = []
        self.promo_claims: list = []

    async def list(self, table, *, field=None, value=None, index=None, **kw):
        self.listed.append((table, field, value))
        return self.rows

    async def update(self, table, id, patch):
        self.updated.append((table, id, patch))
        return {}

    async def create(self, table, doc):
        self.created.append((table, doc))
        return f"{table}-{len(self.created)}"

    async def transition(self, table, id, to, **kw):
        self.transitioned.append((table, id, to))
        return {}

    async def promo_claim(self, claimed_by, claimed_by_user_id, notes=None):
        self.promo_claims.append((claimed_by, claimed_by_user_id, notes))
        return "TESTCODE1"

    async def put_transcript(self, table, id, text):
        self.transcripts = getattr(self, "transcripts", [])
        self.transcripts.append((table, id, text))
        return f"{table}/{id}.txt"


@pytest.fixture
def fake_service(monkeypatch):
    """Install an in-memory FakeService on the ingress module.

    Args:
        monkeypatch: pytest's monkeypatch fixture.

    Returns:
        The installed FakeService instance.
    """
    fake = FakeService()
    monkeypatch.setattr(ing, "_service", fake)
    return fake


def _slack_headers(body: bytes) -> dict:
    """Build valid Slack request headers (v0 HMAC signature) for a raw body.

    Args:
        body: The exact request body bytes the signature is computed over.

    Returns:
        Headers with a current timestamp and matching X-Slack-Signature.
    """
    ts = str(int(time.time()))
    sig = "v0=" + hmac.new(ing.SLACK_SIGNING_SECRET.encode(),
                           b"v0:" + ts.encode() + b":" + body, hashlib.sha256).hexdigest()
    return {"X-Slack-Request-Timestamp": ts, "X-Slack-Signature": sig,
            "Content-Type": "application/json"}


def _mention(text: str, eid: str) -> bytes:
    """Encode a signed-ready app_mention event body.

    Args:
        text: The raw mention text (including the bot mention).
        eid: A unique Slack event id.

    Returns:
        The JSON-encoded event body bytes.
    """
    return json.dumps({
        "type": "event_callback", "event_id": eid,
        "event": {"type": "app_mention", "text": text,
                  "channel": "C1", "ts": "1.2", "user": "U9"},
    }).encode()


# ---------------------------------------------------------------------------
# @bammy routing
# ---------------------------------------------------------------------------

def test_bammy_directive_fast_path_skips_classifier(fake_service, monkeypatch):
    """A mention with an explicit directive creates a bench task with zero
    classifier involvement (the classifier would raise if reached).

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        monkeypatch: pytest's monkeypatch fixture.
    """
    async def boom(*a, **kw):
        raise AssertionError("classifier must not run on the directive fast-path")
    monkeypatch.setattr(bammy, "_classify", boom)
    monkeypatch.setenv("CLAUDE_PROXY_URLS", "http://proxy.test:9090")

    client = TestClient(ing.app)
    body = _mention("<@U1> [canary] build a json parser", "Ev-fast-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    assert fake_service.created, "expected a bench task"
    table, doc = fake_service.created[0]
    assert table == "tasks"
    assert doc["prompt"] == "build a json parser"
    assert doc["bamlChannel"] == "canary"


def test_bammy_no_proxy_falls_back_to_bench(fake_service, monkeypatch):
    """Without CLAUDE_PROXY_URLS the router behaves exactly pre-bammy:
    every mention becomes a bench task.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        monkeypatch: pytest's monkeypatch fixture.
    """
    monkeypatch.delenv("CLAUDE_PROXY_URLS", raising=False)
    client = TestClient(ing.app)
    body = _mention("<@U1> please try building a csv tool", "Ev-fallback-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    table, doc = fake_service.created[0]
    assert table == "tasks"
    assert doc["prompt"] == "please try building a csv tool"


@pytest.fixture
def slack_posts(monkeypatch):
    """Capture bench_core.slack_client.post_message calls.

    Args:
        monkeypatch: pytest's monkeypatch fixture.

    Returns:
        The list of (channel, text, thread_ts) tuples posted.
    """
    posts = []

    async def fake_post(token, channel, text, *, thread_ts=None, blocks=None):
        posts.append((channel, text, thread_ts))
        return "1.99"

    from bench_core import slack_client
    monkeypatch.setattr(slack_client, "post_message", fake_post)
    return posts


def _stub_classifier(monkeypatch, intent: dict):
    """Make the classifier return a canned route.json output.

    Args:
        monkeypatch: pytest's monkeypatch fixture.
        intent: The intent dict the classifier should return.
    """
    monkeypatch.setenv("CLAUDE_PROXY_URLS", "http://proxy.test:9090")

    async def fake_classify(*a, **kw):
        return intent

    monkeypatch.setattr(bammy, "_classify", fake_classify)


BASE_INTENT = {
    "intent": "general", "bench_prompt": "", "version_ref": "", "mode": "revise",
    "guidance": "", "promo_notes": "", "feedback_text": "",
    "needs_clarification": False, "clarification_question": "",
}


def test_bammy_changelog_edit_requeues_entry(fake_service, slack_posts, monkeypatch):
    """A changelog_edit intent resolves the version, patches the row with the
    guidance + slack routing, and requeues it.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        slack_posts: Captured Slack replies.
        monkeypatch: pytest's monkeypatch fixture.
    """
    fake_service.rows = [
        {"_id": "ce-1", "version": "0.222.0", "channel": "canary",
         "date": "2026-06-01", "status": "done", "title": "t", "body": "b"},
    ]
    _stub_classifier(monkeypatch, {
        **BASE_INTENT, "intent": "changelog_edit", "version_ref": "0.222",
        "mode": "revise", "guidance": "mention the new --no-cache flag",
    })
    client = TestClient(ing.app)
    body = _mention("<@U1> fix the 0.222 entry", "Ev-cl-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    assert fake_service.updated, "expected the entry row to be patched"
    _, row_id, patch = fake_service.updated[0]
    assert row_id == "ce-1"
    assert patch["reviseMode"] == "revise"
    assert patch["reviseGuidance"] == "mention the new --no-cache flag"
    assert patch["slackChannel"] == "C1"
    assert ("changelogEntries", "ce-1", "queued") in fake_service.transitioned
    assert any("Revising" in t for _, t, _ in slack_posts)


def test_bammy_promo_claims_code(fake_service, slack_posts, monkeypatch):
    """A promo_claim intent claims a code and replies with it in-thread.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        slack_posts: Captured Slack replies.
        monkeypatch: pytest's monkeypatch fixture.
    """
    _stub_classifier(monkeypatch, {
        **BASE_INTENT, "intent": "promo_claim", "promo_notes": "for Jane",
    })

    async def fake_users_info(token, uid):
        return {"profile": {"display_name": "Jane"}}

    from bench_core import slack_client
    monkeypatch.setattr(slack_client, "users_info", fake_users_info)

    client = TestClient(ing.app)
    body = _mention("<@U1> promo code please", "Ev-promo-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    assert fake_service.promo_claims == [("Jane", "U9", "for Jane")]
    assert any("TESTCODE1" in t for _, t, _ in slack_posts)


def test_bammy_changelog_sync_enqueues_missing(fake_service, slack_posts, monkeypatch):
    """A changelog_sync intent sweeps GitHub and replies with what it enqueued.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        slack_posts: Captured Slack replies.
        monkeypatch: pytest's monkeypatch fixture.
    """
    from bench_core import changelog_github

    monkeypatch.setattr(
        changelog_github, "recent_release_tags",
        lambda channels, limit: ["baml-language-0.11.3-nightly.20260611.a"],
    )
    _stub_classifier(monkeypatch, {**BASE_INTENT, "intent": "changelog_sync"})
    client = TestClient(ing.app)
    body = _mention("<@U1> add new changelog entries", "Ev-sync-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    table, doc = fake_service.created[0]
    assert table == "changelogEntries" and doc["status"] == "queued"
    assert any("Enqueued 1 new entry" in t for _, t, _ in slack_posts)


def test_bammy_posthog_query_answers_with_table(fake_service, slack_posts, monkeypatch):
    """A posthog_query intent turns the question into HogQL, runs it, and
    replies with the query + an aligned result table; unconfigured PostHog
    gets a setup hint instead.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        slack_posts: Captured Slack replies.
        monkeypatch: pytest's monkeypatch fixture.
    """
    from bench_core import posthog_client
    from services.ingress.handlers import posthog_query as ph

    _stub_classifier(monkeypatch, {
        **BASE_INTENT, "intent": "posthog_query",
        "posthog_question": "how many signups this week",
    })

    # Unconfigured: a setup hint, no agent calls.
    monkeypatch.delenv("ATB_POSTHOG_API_KEY", raising=False)
    client = TestClient(ing.app)
    body = _mention("<@U1> how many signups this week", "Ev-ph-0")
    assert client.post("/slack/events", content=body, headers=_slack_headers(body)).status_code == 200
    assert any("isn't wired up" in t for _, t, _ in slack_posts)
    slack_posts.clear()

    # Configured: stub the agent + the query API.
    monkeypatch.setenv("ATB_POSTHOG_API_KEY", "phx_test")
    monkeypatch.setenv("ATB_POSTHOG_PROJECT_ID", "123")

    async def fake_nl_to_hogql(question, catalog, prior_error=None):
        return "SELECT event, count() FROM events LIMIT 5"

    async def fake_hogql(query, **kw):
        return {"columns": ["event", "count()"], "results": [["signup", 42]]}

    async def fake_top_events(days=30, limit=40):
        return [("signup", 42)]

    monkeypatch.setattr(ph, "_nl_to_hogql", fake_nl_to_hogql)
    monkeypatch.setattr(posthog_client, "hogql", fake_hogql)
    monkeypatch.setattr(posthog_client, "top_events", fake_top_events)

    body = _mention("<@U1> how many signups this week", "Ev-ph-1")
    assert client.post("/slack/events", content=body, headers=_slack_headers(body)).status_code == 200
    replies = [t for _, t, _ in slack_posts]
    assert any("SELECT event" in t and "signup" in t and "42" in t for t in replies)


def test_bammy_feedback_creates_task_and_trophy(fake_service, slack_posts, monkeypatch):
    """A feedback intent creates the done task + queued trophy pair.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        slack_posts: Captured Slack replies.
        monkeypatch: pytest's monkeypatch fixture.
    """
    _stub_classifier(monkeypatch, {
        **BASE_INTENT, "intent": "feedback",
        "feedback_text": "baml fmt is slow on big files",
    })
    client = TestClient(ing.app)
    body = _mention("<@U1> feedback: baml fmt is slow", "Ev-fb-1")
    r = client.post("/slack/events", content=body, headers=_slack_headers(body))
    assert r.status_code == 200
    tables = [t for t, _ in fake_service.created]
    assert tables == ["tasks", "trophies"]
    task_doc = fake_service.created[0][1]
    trophy_doc = fake_service.created[1][1]
    assert task_doc["source"] == "feedback" and task_doc["status"] == "done"
    assert trophy_doc["source"] == "feedback" and trophy_doc["status"] == "queued"
    assert trophy_doc["outcome"] == "feedback"
    assert any("Logged" in t for _, t, _ in slack_posts)


# ---------------------------------------------------------------------------
# public ingress endpoints
# ---------------------------------------------------------------------------

def test_entries_endpoint_shapes_published_rows(fake_service):
    """GET /entries returns the website contract: {"entries": [...]} with the
    six public fields, newest-first by date.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
    """
    fake_service.rows = [
        {"_id": "a", "version": "0.221.0", "channel": "canary", "date": "2026-05-01",
         "title": "old", "body": "b1", "authors": ["x"], "status": "done",
         "createdAt": 1, "meta": {"scores": {}}, "reviseMode": ""},
        {"_id": "b", "version": "0.222.0", "channel": "canary", "date": "2026-06-01",
         "title": "new", "body": "b2", "authors": ["y"], "status": "done",
         "createdAt": 2, "claimedBy": "secret-worker-id"},
    ]
    client = TestClient(ing.app)
    r = client.get("/entries")
    assert r.status_code == 200
    entries = r.json()["entries"]
    assert [e["version"] for e in entries] == ["0.222.0", "0.221.0"]
    # only the public fields leak out (no queue internals)
    assert set(entries[0].keys()) == {"version", "date", "title", "body", "authors", "channel"}


def test_feedback_endpoint_creates_pair_and_rate_limits(fake_service):
    """POST /feedback creates the task+trophy pair; hammering it trips the
    per-IP token bucket with 429.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
    """
    ing._fb_buckets.clear()
    ing._fb_day[1] = 0
    client = TestClient(ing.app)
    r = client.post("/feedback", json={
        "message": "the fmt command is slow", "bamlVersion": "0.222.0",
        "os": "darwin", "arch": "arm64",
    })
    assert r.status_code == 200
    assert set(r.json().keys()) == {"taskId", "trophyId"}
    trophy_doc = fake_service.created[1][1]
    assert trophy_doc["metrics"]["bamlVersion"] == "0.222.0"

    # burst cap is 3; the 4th immediate request is rate-limited
    for _ in range(2):
        assert client.post("/feedback", json={"message": "x"}).status_code == 200
    assert client.post("/feedback", json={"message": "x"}).status_code == 429


def test_feedback_endpoint_accepts_team_context(fake_service):
    """With the team opt-in payload (transcript + filesCreated), the trophy
    carries the blob pointer and the files; without it neither appears.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
    """
    ing._fb_buckets.clear()
    ing._fb_day[1] = 0
    client = TestClient(ing.app)
    r = client.post("/feedback", json={
        "message": "parser crashes on this input",
        "bamlVersion": "0.222.0",
        "transcript": "not-a-real-session-log\njust text\n",
        "filesCreated": {"baml_src/main.baml": "function F() -> int { 1 }"},
    })
    assert r.status_code == 200
    # transcript stored as a blob on the task, pointer on the trophy
    table, task_id, text = fake_service.transcripts[0]
    assert table == "tasks" and "not-a-real-session-log" in text
    trophy_doc = fake_service.created[1][1]
    assert trophy_doc["transcriptStorageId"] == f"tasks/{task_id}.txt"
    assert trophy_doc["filesCreated"] == {"baml_src/main.baml": "function F() -> int { 1 }"}
    assert trophy_doc["metrics"]["files_touched"] == 1

    # default shape: no context fields on the trophy
    r2 = client.post("/feedback", json={"message": "just an issue + repro in text"})
    assert r2.status_code == 200
    lean_doc = fake_service.created[3][1]
    assert lean_doc.get("transcriptStorageId") is None
    assert lean_doc.get("filesCreated") is None


def test_entries_update_enqueues_only_missing_releases(fake_service, monkeypatch):
    """POST /entries/update sweeps GitHub and enqueues queued rows only for
    releases without an entry (any status counts as present).

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
        monkeypatch: pytest's monkeypatch fixture.
    """
    from bench_core import changelog_github

    monkeypatch.setattr(
        changelog_github, "recent_release_tags",
        lambda channels, limit: [
            "baml-language-0.11.3-nightly.20260611.a",  # newest, missing
            "baml-language-0.222.0",                     # exists (failed counts)
        ],
    )
    fake_service.rows = [
        {"_id": "ce-1", "version": "0.222.0", "channel": "canary", "status": "failed"},
    ]
    client = TestClient(ing.app)
    r = client.post("/entries/update",
                    headers={"Authorization": "Bearer devservicetoken"})
    assert r.status_code == 200
    enq = r.json()["enqueued"]
    assert [e["version"] for e in enq] == ["0.11.3-nightly.20260611.a"]
    table, doc = fake_service.created[0]
    assert table == "changelogEntries"
    assert doc["status"] == "queued" and doc["channel"] == "nightly"
    # auth required
    assert client.post("/entries/update").status_code == 401


def test_feedback_endpoint_validates_message(fake_service):
    """POST /feedback rejects an empty message with 400.

    Args:
        fake_service: In-memory ServiceClient fake on the ingress module.
    """
    ing._fb_buckets.clear()
    ing._fb_day[1] = 0
    client = TestClient(ing.app)
    assert client.post("/feedback", json={"message": "  "}).status_code == 400


# ---------------------------------------------------------------------------
# api surfaces against the MemoryGateway
# ---------------------------------------------------------------------------

@pytest.fixture
def api_client(monkeypatch):
    """A TestClient for the api app backed by the in-process MemoryGateway.

    Args:
        monkeypatch: pytest's monkeypatch fixture.

    Returns:
        A TestClient sending the dev bearer token on every request.
    """
    monkeypatch.setenv("CONVEX_BACKEND", "memory")
    from services.api.app import create_app
    app = create_app()
    client = TestClient(app)
    client.headers["Authorization"] = "Bearer devservicetoken"
    return client


def test_promo_claim_issues_codes_in_position_order(api_client):
    """POST /promo/claim hands out unused codes lowest-position-first and
    returns null when inventory is exhausted.

    Args:
        api_client: Memory-backed api TestClient.
    """
    for code, pos in [("BBB", 2), ("AAA", 1)]:
        r = api_client.post("/promoCodes", json={
            "code": code, "position": pos, "status": "unused",
        })
        assert r.status_code == 200
    claim = {"claimedBy": "Jane", "claimedByUserId": "U9", "notes": "test"}
    assert api_client.post("/promo/claim", json=claim).json()["code"] == "AAA"
    assert api_client.post("/promo/claim", json=claim).json()["code"] == "BBB"
    assert api_client.post("/promo/claim", json=claim).json()["code"] is None
    used = api_client.get("/promoCodes", params={"field": "status", "value": "used"}).json()
    assert {r["claimedBy"] for r in used} == {"Jane"}


def test_workers_heartbeat_upserts_presence(api_client):
    """POST /workers/heartbeat inserts then updates (not duplicates) a row.

    Args:
        api_client: Memory-backed api TestClient.
    """
    beat = {"workerId": "baml_worker-host-1-abc", "role": "baml_worker",
            "status": "busy", "currentItemId": "task-1"}
    assert api_client.post("/workers/heartbeat", json=beat).status_code == 204
    assert api_client.post("/workers/heartbeat", json={**beat, "status": "idle",
                                                       "currentItemId": None}).status_code == 204
    rows = api_client.get("/workers").json()
    assert len(rows) == 1
    assert rows[0]["status"] == "idle"
    assert "currentItemId" not in rows[0]


def test_changelog_entries_queue_roundtrip(api_client):
    """changelogEntries supports the full queue verbs: create queued ->
    claim into generating -> transition done with the entry patch.

    Args:
        api_client: Memory-backed api TestClient.
    """
    r = api_client.post("/changelogEntries", json={
        "version": "0.222.0", "tag": "baml-language-0.222.0",
        "channel": "canary", "status": "queued",
    })
    row_id = r.json()["id"]
    claimed = api_client.post("/changelogEntries/claim", json={
        "workerId": "cw-1", "leaseMs": 60000, "field": "status",
        "value": "queued", "claimedValue": "generating",
    }).json()
    assert claimed["_id"] == row_id and claimed["status"] == "generating"
    done = api_client.post(f"/changelogEntries/{row_id}/transition", json={
        "to": "done", "patch": {"title": "T", "body": "B", "date": "2026-06-10"},
    }).json()
    assert done["status"] == "done" and done["title"] == "T"


def test_wasm_upload_validates_and_serves(api_client, tmp_path, monkeypatch):
    """PUT /wasm rejects junk, accepts a real tarball, and the public GET
    serves it back with the website cache header.

    Args:
        api_client: Memory-backed api TestClient.
        tmp_path: pytest tmp dir used as the blob volume.
        monkeypatch: pytest's monkeypatch fixture.
    """
    import io
    import tarfile

    from services.api import blobs
    monkeypatch.setattr(blobs, "BLOB_DIR", tmp_path)

    assert api_client.get("/wasm/bridge_wasm.tar.gz").status_code == 404
    assert api_client.put("/wasm/bridge_wasm.tar.gz", content=b"junk").status_code == 400

    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tf:
        for name, data in [("SOURCE_HASH", b"123-456\n"), ("bridge_wasm_bg.wasm", b"\0asm")]:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tf.addfile(info, io.BytesIO(data))
    tarball = buf.getvalue()
    r = api_client.put("/wasm/bridge_wasm.tar.gz", content=tarball)
    assert r.status_code == 200 and r.json()["sizeBytes"] == len(tarball)

    got = api_client.get("/wasm/bridge_wasm.tar.gz")
    assert got.status_code == 200
    assert got.headers["cache-control"] == "public, max-age=300"
    assert got.content == tarball
