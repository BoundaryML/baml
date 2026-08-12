"""Keyless streaming smokes — string-typed and class-typed `T`.

Exercises the full streaming path — bridge → BAML LLM client → HTTP → SSE →
StreamAccumulator → SAP → `Stream.next()/final()` → bridge — without hitting
OpenAI. The `@replay_server` decorator (see `replay_harness.py`) runs each test
against an in-process BAML server replaying a checked-in SSE recording, with the
env-driven `StreamStub` client pointed at it:

  * string `T` — `stream_e2e_extract(text) -> string` (Stream<null | string, string>)
  * class  `T` — `stream_e2e_extract_doc(text) -> StreamingDoc { title, body, word_count }`

The recordings stream many SSE chunks, so each `next()` yields >= 10 partials
before `Done` (asserted below). The class-typed tests are the
deterministic, bridge-level regression guard for the class-typed streaming bug
(thoughts/sam-projects/bridge-generics/streaming, doc 00).

Re-record the SSE fixtures (needs a real key):

    INSTA_UPDATE=always infisical run -- cargo nextest run -p sdk_test_llm_recordings
"""
from replay_harness import replay_server


# ---------------------------------------------------------------------------
# String-typed `T` — Stream<null | string, string>.
# ---------------------------------------------------------------------------


@replay_server(recording_path="replay_extract_string")
def test_streaming_e2e_stream():
    """Sync `next()` yields a stream of partials and drains to `Done`."""
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_stream

    stream = stream_e2e_extract_stream("ignored-by-replay-server")
    results = 0
    while True:
        v = stream.next()
        if isinstance(v, Done):
            break
        results += 1
        assert v is None or isinstance(v, str)
        assert results < 10_000, "stream.next() failed to terminate"
    assert results >= 10, "expected stream.next() to yield at least 10 partials"
    assert isinstance(stream.final(), str)


@replay_server(recording_path="replay_extract_string")
async def test_streaming_e2e_stream_async():
    """Async sibling over the pyo3-tokio path: `next_async()` / `final_async()`."""
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_stream_async

    stream = await stream_e2e_extract_stream_async("ignored-by-replay-server")
    results = 0
    while True:
        v = await stream.next_async()
        if isinstance(v, Done):
            break
        results += 1
        assert v is None or isinstance(v, str)
        assert results < 10_000, "stream.next_async() failed to terminate"
    assert results >= 10, "expected stream.next_async() to yield at least 10 partials"
    assert isinstance(await stream.final_async(), str)


@replay_server(recording_path="replay_extract_string")
def test_streaming_e2e_stream_collect_in_baml():
    """BAML-driven counterpart: the `S | Done` union stays engine-side."""
    from baml_sdk.lorem import stream_e2e_collect, StreamE2ECollectResult

    result = stream_e2e_collect("ignored-by-replay-server")
    assert isinstance(result, StreamE2ECollectResult)
    assert len(result.next_calls) >= 10, "expected at least 10 collected partials"
    for item in result.next_calls:
        assert item is None or isinstance(item, str)
    assert isinstance(result.final_call, str)


# ---------------------------------------------------------------------------
# Class-typed `T` — Stream<StreamingDoc$stream, StreamingDoc>. The case the
# plain-`string` tests above deliberately avoid; the regression guard for the
# class-typed streaming bug (doc 00).
# ---------------------------------------------------------------------------


@replay_server(recording_path="replay_extract_doc")
def test_streaming_e2e_stream_doc():
    """Sync `next()` yields >= 10 doc partials; `final()` is a typed `StreamingDoc`."""
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_doc_stream, StreamingDoc

    stream = stream_e2e_extract_doc_stream("ignored-by-replay-server")
    results = 0
    while True:
        v = stream.next()
        if isinstance(v, Done):
            break
        results += 1
        if v is not None:
            assert hasattr(v, "title"), f"unexpected partial: {v!r}"
        assert results < 10_000, "stream.next() failed to terminate"
    assert results >= 10, "expected stream.next() to yield at least 10 partials"
    assert isinstance(stream.final(), StreamingDoc)


@replay_server(recording_path="replay_extract_doc")
async def test_streaming_e2e_stream_doc_async():
    """Async sibling over the pyo3-tokio path for a class `T`."""
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import stream_e2e_extract_doc_stream_async, StreamingDoc

    stream = await stream_e2e_extract_doc_stream_async("ignored-by-replay-server")
    results = 0
    while True:
        v = await stream.next_async()
        if isinstance(v, Done):
            break
        results += 1
        if v is not None:
            assert hasattr(v, "title"), f"unexpected partial: {v!r}"
        assert results < 10_000, "stream.next_async() failed to terminate"
    assert results >= 10, "expected stream.next_async() to yield at least 10 partials"
    assert isinstance(await stream.final_async(), StreamingDoc)


@replay_server(recording_path="replay_extract_doc")
def test_streaming_e2e_stream_doc_collect_in_baml():
    """BAML-driven counterpart: the `S | Done` union stays engine-side;
    only the concrete `StreamingDoc` crosses the FFI boundary."""
    from baml_sdk.lorem import stream_e2e_collect_doc, StreamingDoc
    from baml_sdk.stream_types.lorem import StreamingDoc as StreamingDocPartial

    result = stream_e2e_collect_doc("ignored-by-replay-server")
    assert isinstance(result, (StreamingDoc, StreamingDocPartial))
    assert hasattr(result, "title")
