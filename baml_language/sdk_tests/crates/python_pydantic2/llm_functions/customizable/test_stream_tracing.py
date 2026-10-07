"""Python streams retain their creating invocation across host pulls."""

import asyncio

import pytest
from replay_harness import replay_server


# SDK_PARITY_LINT(skip): Python stream lifetime/context regression
@replay_server(recording_path="replay_extract_string")
async def test_stream_async_for_keeps_context_after_creator_returns():
    from baml_sdk import trace
    from baml_sdk.lorem import trace_stream_string_stream_async

    contexts = []

    def on_event(_event):
        contexts.append(trace.current_context())

    @trace.instrument(
        trace.span(error=True).context(
            metadata={"feature": "creator"}, distinct_id="u1"
        )
    )
    async def create():
        return await trace_stream_string_stream_async("Ada", on_event=on_event)

    stream = await create()
    assert trace.current_context().metadata == {}
    partials = [value async for value in stream]
    assert len(partials) >= 10
    assert isinstance(await stream.final_async(), str)
    assert len(contexts) >= 3
    assert all(context.metadata == {"feature": "creator"} for context in contexts)
    assert all(context.distinct_id == "u1" for context in contexts)
    assert trace.current_context().metadata == {}


# SDK_PARITY_LINT(skip): Python concurrent stream context isolation
@replay_server(recording_path="replay_extract_string")
async def test_stream_contexts_are_isolated_during_concurrent_iteration():
    from baml_sdk import trace
    from baml_sdk.lorem import trace_stream_string_stream_async

    async def consume(feature):
        seen = []

        def on_event(_event):
            seen.append(trace.current_context().metadata)

        stream = await trace_stream_string_stream_async(
            "Ada",
            on_event=on_event,
            _baml={"trace": trace.context(metadata={"feature": feature})},
        )
        async for _ in stream:
            await asyncio.sleep(0)
        assert isinstance(await stream.final_async(), str)
        assert len(seen) >= 3
        assert all(metadata == {"feature": feature} for metadata in seen)

    await asyncio.gather(consume("first"), consume("second"))
    assert trace.current_context().metadata == {}


# SDK_PARITY_LINT(skip): Python sync stream defaults and explicit step patches
@replay_server(recording_path="replay_extract_string")
def test_stream_final_keeps_context_and_allows_a_step_patch():
    from baml_sdk import trace
    from baml_sdk.lorem import trace_stream_string_stream

    seen = []

    def on_event(_event):
        seen.append(trace.current_context().metadata)

    stream = trace_stream_string_stream(
        "Ada",
        on_event=on_event,
        _baml={"trace": trace.context(metadata={"feature": "origin", "user": "u1"})},
    )
    final = stream.final(_baml={"trace": trace.context(metadata={"feature": "step"})})
    assert isinstance(final, str)
    assert seen[0] == {"feature": "origin", "user": "u1"}
    assert all(metadata == {"feature": "step", "user": "u1"} for metadata in seen[1:])
    assert trace.current_context().metadata == {}


# SDK_PARITY_LINT(skip): Python EOF settlement retains final parse errors
@replay_server(recording_path="replay_extract_string")
async def test_stream_final_parse_failure_is_preserved_after_eof():
    from baml_bridge import BamlError
    from baml_sdk.ai.errors import ParseFailed
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import trace_stream_doc_stream_async

    stream = await trace_stream_doc_stream_async("Ada")
    while not isinstance(await stream.next_async(), Done):
        pass
    for _ in range(2):
        with pytest.raises(BamlError) as raised:
            await stream.final_async()
        assert isinstance(raised.value.value, ParseFailed)
        assert raised.value.value.raw_output


@pytest.mark.parametrize(
    "kind,cache",
    [
        ("interrupt", False),
        ("exit", False),
        ("task_cancel", False),
        ("baml_cancel", False),
        ("error", True),
        ("panic", True),
    ],
)
# SDK_PARITY_LINT(skip): Python final caching must distinguish results from control flow
def test_stream_final_caches_only_settled_errors(monkeypatch, kind, cache):
    from baml_bridge import BamlCancelledError, BamlError, BamlPanic
    from baml_bridge._stream import BamlStream

    errors = {
        "interrupt": KeyboardInterrupt(),
        "exit": SystemExit(),
        "task_cancel": asyncio.CancelledError(),
        "baml_cancel": BamlCancelledError("cancelled"),
        "error": BamlError("parse failed"),
        "panic": BamlPanic("panic"),
    }
    error = errors[kind]
    stream = BamlStream(None)
    calls = []
    finishes = []

    class Execution:
        def finish(self, outcome, value):
            finishes.append((outcome, value))

    stream._execution = Execution()

    def call(_self, _fqn, *, _baml):
        calls.append(_fqn)
        if len(calls) == 1:
            raise error
        return "finished"

    monkeypatch.setattr(BamlStream, "_call_sync", call)
    with pytest.raises(type(error)) as raised:
        stream.final()
    assert raised.value is error
    assert len(finishes) == 1
    assert stream._settled is cache
    if cache:
        with pytest.raises(type(error)) as raised:
            stream.final()
        assert raised.value is error
        assert len(calls) == 1
    else:
        assert stream.final() == "finished"
        assert len(calls) == 2


# SDK_PARITY_LINT(skip): Python task cancellation must not poison later final awaits
async def test_stream_final_timeout_is_not_cached(monkeypatch):
    from baml_bridge._stream import BamlStream

    stream = BamlStream(None)
    calls = []
    finishes = []

    class Execution:
        def finish(self, outcome, value):
            finishes.append(outcome)

    stream._execution = Execution()

    async def call(_self, _fqn, *, _baml):
        calls.append(_fqn)
        if len(calls) == 1:
            await asyncio.Event().wait()
        return "finished"

    monkeypatch.setattr(BamlStream, "_call_async", call)
    with pytest.raises(TimeoutError):
        await asyncio.wait_for(stream.final_async(), timeout=0.01)
    assert finishes == ["cancelled"]
    assert not stream._settled
    assert stream._final_error is None
    assert await stream.final_async() == "finished"
    assert len(calls) == 2


# SDK_PARITY_LINT(skip): Python stream cancellation survives constructor return
@replay_server(recording_path="replay_extract_string")
async def test_stream_keeps_creation_cancel_token():
    from baml_sdk.baml.spawn import CancelToken
    from baml_sdk.lorem import trace_stream_string_stream_async

    token = CancelToken.new()
    stream = await trace_stream_string_stream_async("Ada", _baml={"cancel": token})
    token.cancel()
    with pytest.raises(asyncio.CancelledError):
        await stream.next_async()


# SDK_PARITY_LINT(skip): Python stream reservations must survive rejected admission
@replay_server(recording_path="replay_extract_string")
async def test_stream_precancelled_admission_preserves_reservation():
    from baml_sdk import trace
    from baml_sdk.baml.spawn import CancelToken
    from baml_sdk.lorem import trace_stream_string_stream_async

    reserved = trace.span(error=True).reserve()
    token = CancelToken.new()
    token.cancel()
    with pytest.raises(asyncio.CancelledError):
        await trace_stream_string_stream_async(
            "Ada", _baml={"trace": reserved, "cancel": token}
        )
    stream = await trace_stream_string_stream_async("Ada", _baml={"trace": reserved})
    assert isinstance(await stream.final_async(), str)


# The ordinary SDK CI does not build the query CLI. Run this audit wherever
# baml-cli is built, or set BAML_AUDIT_CLI to a CLI from the same checkout.
# SDK_PARITY_LINT(skip): native Python recording ancestry, duration, values and step-count audit
def test_stream_recording_parent_chain(tmp_path):
    import json
    import os
    from pathlib import Path
    import subprocess
    import sys
    import uuid

    sdk_root = Path(__file__).resolve().parent
    workspace = next(
        parent.parent for parent in sdk_root.parents if parent.name == "sdk_tests"
    )
    cli = Path(os.environ.get("BAML_AUDIT_CLI", workspace / "target/debug/baml-cli"))
    if not cli.is_file():
        pytest.skip("Build baml_cli to run the native stream recording audit")
    import baml_sdk

    marker = "stream-audit-" + uuid.uuid4().hex
    env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("BOUNDARY_")
    }
    env.update(
        BOUNDARY_API_KEY="local",
        BAML_TELEMETRY="medium",
        BAML_HOME=str(tmp_path / "config"),
        DO_NOT_TRACK="1",
    )
    env["PYTHONPATH"] = str(Path(baml_sdk.__file__).resolve().parent.parent)
    recording_directory = Path.home() / ".baml/btel/recordings"
    before = (
        set(recording_directory.iterdir()) if recording_directory.exists() else set()
    )
    child = subprocess.run(
        [sys.executable, str(sdk_root / "stream_recording_cases.py"), marker],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert child.returncode == 0, child.stderr
    new_recordings = set(recording_directory.iterdir()) - before
    assert new_recordings, "child wrote no local recording"
    recording_filter = " OR ".join(
        f"s.span_id LIKE '{path.name}:%'" for path in new_recordings
    )
    sql = f"""SELECT s.span_id, s.parent_span_id, s.span_name, s.span_type, s.status,
        s.start_time, s.end_time, s.context_metadata, s.context_distinct_id,
        s.error_value, p.span_name AS parent, p.parent_span_id AS grandparent_id,
        g.span_name AS grandparent
        FROM spans s LEFT JOIN spans p ON p.span_id = s.parent_span_id
        LEFT JOIN spans g ON g.span_id = p.parent_span_id
        WHERE ({recording_filter}) AND CAST(s.context_metadata AS TEXT) LIKE '%{marker}%'"""
    result = subprocess.run(
        [
            str(cli),
            "--agent-skill-check",
            "off",
            "query",
            "--local",
            "--from",
            str(Path.home()),
            "--format",
            "json",
            sql,
        ],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, result.stderr + result.stdout
    data = json.loads(result.stdout)
    assert data["outcome"]["status"] == "complete"
    rows = [
        dict(zip((column["name"] for column in data["columns"]), row))
        for row in data["rows"]
    ]
    origins = {
        row["context_metadata"]["case"]: row
        for row in rows
        if row["span_type"] == "function" and row["span_name"].endswith("@stream")
    }
    assert set(origins) == {
        "escaped",
        "root",
        "concurrent-string",
        "concurrent-doc",
        "abandoned",
        "parse-error",
    }
    networks = [row for row in rows if row["span_type"] == "network_span"]
    assert len(networks) == 5
    by_id = {row["span_id"]: row for row in rows}
    for request in networks:
        origin = origins[request["context_metadata"]["case"]]
        assert request["parent"] == "ai.clients.retry_attempt"
        ancestors = set()
        parent_id = request["parent_span_id"]
        while parent_id in by_id and parent_id not in ancestors:
            ancestors.add(parent_id)
            parent_id = by_id[parent_id]["parent_span_id"]
        assert origin["span_id"] in ancestors
        assert (
            origin["start_time"]
            <= request["start_time"]
            <= request["end_time"]
            <= origin["end_time"]
        )
        assert request["context_distinct_id"] == "u1"
    assert not str(origins["root"]["parent"]).startswith("python:")
    assert origins["escaped"]["parent"].startswith("python:")
    assert not any(
        row["span_type"] == "function"
        and row["span_name"] in ("ai.stream.Stream.next", "ai.stream.Stream.final")
        for row in rows
    ), "pulls stay timing-only even when final parsing fails"
    for case in ("concurrent-doc", "parse-error"):
        assert origins[case]["status"] == "user_error"
        error = origins[case]["error_value"]
        assert "ParseFailed" in json.dumps(error)
        assert "raw_output" in json.dumps(error)
    assert origins["abandoned"]["status"] == "cancel_error"
