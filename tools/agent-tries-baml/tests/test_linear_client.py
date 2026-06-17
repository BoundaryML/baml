"""Unit tests for the Linear GraphQL client.

Cover the label-swap read-modify-write (set/update read the current labels, drop
the status-group label, add the new one, write the full set), the create payload
shape (teamId + labelIds + Markdown description), status-name reads, comment
pagination, and adopt-by-title (single vs ambiguous). The Linear API is mocked
with respx, so these run fast with no network.
"""

import json

import httpx
import pytest
import respx

from bench_core import linear_client as lc
from bench_core.linear_client import LinearClient, LinearError

URL = lc.LINEAR_API


def _ok(data: dict) -> httpx.Response:
    """Wrap a GraphQL ``data`` payload in a 200 response."""
    return httpx.Response(200, json={"data": data})


# ---- pure helpers (no network) ----

def test_status_label_name_finds_group_label():
    """status_label_name returns the canonical name of the status-group label."""
    ids = ["some-other-label", lc.LINEAR_STATUS_APPROVED]
    assert lc.status_label_name(ids) == "approved"
    assert lc.status_label_name(["some-other-label"]) is None


def test_issue_body_md_renders_all_sections():
    """The Markdown body carries links, category, repro, suggestion, and evidence."""
    body = lc.issue_body_md(
        "A bug.", ["https://dash/runs/t1?call=3"], suggestion="Do the fix.",
        category="bug", repro="class A {}",
        issue_link="https://dash/issues/i1", pr_url="https://gh/o/r/pull/9",
    )
    assert "**Links:**" in body
    assert "[Issue](https://dash/issues/i1)" in body and "[PR](https://gh/o/r/pull/9)" in body
    assert "**Category:** bug" in body
    assert "A bug." in body
    assert "## Reproduction" in body and "```typescript" in body and "class A {}" in body
    assert "## Suggested fix" in body and "Do the fix." in body
    assert "## Evidence" in body and "call 3" in body


def test_issue_body_md_omits_absent_sections():
    """With nothing optional supplied, only the description is rendered."""
    assert lc.issue_body_md("Just a description.", []) == "Just a description."


def test_swap_status_label_keeps_others_and_replaces_status():
    """_swap_status_label drops the existing status label, keeps the rest, adds new."""
    out = LinearClient._swap_status_label(
        ["keep-me", lc.LINEAR_STATUS_NOT_STARTED], lc.LINEAR_STATUS_APPROVED)
    assert out == ["keep-me", lc.LINEAR_STATUS_APPROVED]
    # exactly one status-group label survives
    assert [x for x in out if x in lc.STATUS_GROUP_LABEL_IDS] == [lc.LINEAR_STATUS_APPROVED]


# ---- create ----

@respx.mock
async def test_create_issue_sends_team_label_and_returns_id():
    """create_issue posts teamId + the status label + a Markdown description."""
    route = respx.post(URL).mock(return_value=_ok(
        {"issueCreate": {"success": True, "issue": {"id": "li_new"}}}))
    c = LinearClient("key")
    iid = await c.create_issue(
        "Add modulo", lc.LINEAR_STATUS_NOT_STARTED, "It breaks.",
        ["https://dash/runs/t1?call=2"], suggestion="fix it", category="bug",
        repro="x % y", issue_link="https://dash/issues/i1", pr_url=None)
    assert iid == "li_new"
    inp = json.loads(route.calls.last.request.content)["variables"]["input"]
    assert inp["teamId"] == lc.LINEAR_TEAM_ID
    assert inp["title"] == "Add modulo"
    assert inp["labelIds"] == [lc.LINEAR_STATUS_NOT_STARTED]
    assert "It breaks." in inp["description"] and "## Reproduction" in inp["description"]


@respx.mock
async def test_create_issue_raises_on_failure():
    """A success=false response raises LinearError rather than returning a bad id."""
    respx.post(URL).mock(return_value=_ok({"issueCreate": {"success": False, "issue": None}}))
    with pytest.raises(LinearError):
        await LinearClient("key").create_issue("t", lc.LINEAR_STATUS_NOT_STARTED, "b", [])


@respx.mock
async def test_gql_errors_raise():
    """A GraphQL errors array surfaces as a LinearError."""
    respx.post(URL).mock(return_value=httpx.Response(200, json={"errors": [{"message": "nope"}]}))
    with pytest.raises(LinearError):
        await LinearClient("key").create_issue("t", lc.LINEAR_STATUS_NOT_STARTED, "b", [])


# ---- label-swap read-modify-write ----

def _swap_router(current_label_ids):
    """A respx side_effect: answer the labels read, then capture the issueUpdate."""
    captured: dict = {}

    def handler(request):
        body = json.loads(request.content)
        if "issueUpdate" in body["query"]:
            captured["update"] = body["variables"]
            return _ok({"issueUpdate": {"success": True}})
        return _ok({"issue": {"labels": {"nodes": [{"id": i} for i in current_label_ids]}}})

    return handler, captured


@respx.mock
async def test_set_status_swaps_only_the_status_label():
    """set_status reads current labels, drops the status one, keeps the rest, adds new."""
    handler, captured = _swap_router(["keep-me", lc.LINEAR_STATUS_NOT_STARTED])
    respx.post(URL).mock(side_effect=handler)
    await LinearClient("key").set_status("li_1", lc.LINEAR_STATUS_TO_CURSOR)
    label_ids = captured["update"]["input"]["labelIds"]
    assert set(label_ids) == {"keep-me", lc.LINEAR_STATUS_TO_CURSOR}
    assert [x for x in label_ids if x in lc.STATUS_GROUP_LABEL_IDS] == [lc.LINEAR_STATUS_TO_CURSOR]
    # set_status touches labels only — never the title/description
    assert "title" not in captured["update"]["input"]


@respx.mock
async def test_update_issue_rewrites_body_and_swaps_label():
    """update_issue writes title + description + the swapped label set in one update."""
    handler, captured = _swap_router([lc.LINEAR_STATUS_NOT_STARTED])
    respx.post(URL).mock(side_effect=handler)
    await LinearClient("key").update_issue(
        "li_1", "New title", lc.LINEAR_STATUS_APPROVED, "Updated body.", [])
    inp = captured["update"]["input"]
    assert inp["title"] == "New title"
    assert "Updated body." in inp["description"]
    assert inp["labelIds"] == [lc.LINEAR_STATUS_APPROVED]


# ---- reads ----

@respx.mock
async def test_get_status_returns_label_name():
    """get_status maps the issue's status-group label id back to its name."""
    respx.post(URL).mock(return_value=_ok(
        {"issue": {"labels": {"nodes": [{"id": "x"}, {"id": lc.LINEAR_STATUS_REDRAFT}]}}}))
    assert await LinearClient("key").get_status("li_1") == "redraft"


@respx.mock
async def test_get_comments_paginates_and_normalizes():
    """get_comments follows pageInfo and returns {text, created_time, author} dicts."""
    pages = iter([
        _ok({"issue": {"comments": {
            "nodes": [{"body": "first", "createdAt": "2026-06-15T00:00:00Z",
                       "user": {"id": "u1"}}],
            "pageInfo": {"hasNextPage": True, "endCursor": "c1"}}}}),
        _ok({"issue": {"comments": {
            "nodes": [{"body": "second", "createdAt": "2026-06-15T01:00:00Z",
                       "user": {"id": "u2"}}],
            "pageInfo": {"hasNextPage": False, "endCursor": None}}}}),
    ])
    respx.post(URL).mock(side_effect=lambda request: next(pages))
    out = await LinearClient("key").get_comments("li_1")
    assert [c["text"] for c in out] == ["first", "second"]
    assert out[0] == {"text": "first", "created_time": "2026-06-15T00:00:00Z", "author": "u1"}


# ---- adopt-by-title ----

@respx.mock
async def test_find_issue_by_title_single_status_bearing_match():
    """A single status-bearing exact-title match is adopted (its id returned)."""
    respx.post(URL).mock(return_value=_ok({"issues": {"nodes": [
        {"id": "li_match", "labels": {"nodes": [{"id": lc.LINEAR_STATUS_NOT_STARTED}]}},
    ]}}))
    assert await LinearClient("key").find_issue_by_title("Add modulo") == "li_match"


@respx.mock
async def test_find_issue_by_title_ignores_non_status_issues():
    """A title match that carries no status-group label is not adopted."""
    respx.post(URL).mock(return_value=_ok({"issues": {"nodes": [
        {"id": "li_other", "labels": {"nodes": [{"id": "unrelated-label"}]}},
    ]}}))
    assert await LinearClient("key").find_issue_by_title("Add modulo") is None


@respx.mock
async def test_find_issue_by_title_ambiguous_returns_none():
    """More than one status-bearing match is ambiguous -> skip (None), never guess."""
    respx.post(URL).mock(return_value=_ok({"issues": {"nodes": [
        {"id": "li_a", "labels": {"nodes": [{"id": lc.LINEAR_STATUS_NOT_STARTED}]}},
        {"id": "li_b", "labels": {"nodes": [{"id": lc.LINEAR_STATUS_APPROVED}]}},
    ]}}))
    assert await LinearClient("key").find_issue_by_title("Add modulo") is None
