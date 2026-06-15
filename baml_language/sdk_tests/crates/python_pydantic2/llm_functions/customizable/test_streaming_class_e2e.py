"""End-to-end streaming against OpenAI for *class-typed* `T` — the case
the plain-`string` `test_streaming_e2e.py` deliberately avoids.

`StreamE2EExtract` returns `string`, so its synthesized `$stream`
companion is `Stream<string, null | string>` — the one streaming shape
the docs claim the engine accepts today. These tests instead drive
functions whose return type is a *class with multiple fields*:

    StreamE2EExtractDoc(text)    -> StreamingDoc { title, body, word_count }
    StreamE2EExtractResume(text) -> Resume       { name, email }

so the companion is `Stream<StreamingDoc$stream, StreamingDoc>` /
`Stream<Resume$stream, Resume>`. Both use the `StreamStub` (openai)
client so the request actually reaches a streaming-capable provider —
unlike `StreamingExtract`, whose `openai-responses` provider rejects
streaming outright.

These originally reproduced the `Non-parsable type: Void` failure at
`StreamCache.new` (emit-phase namespace-resolution bug in
`compute_stream_return_type`, since fixed) and now guard the class-typed
streaming path. The investigation writeup lives in
`thoughts/sam-projects/bridge-generics/streaming`.

Skipped unless OPENAI_API_KEY is set. Run via:

    cargo nextest run -p sdk_test_python_pydantic2 llm_functions::pytest
    (cd .../llm_functions/generated && infisical run -- uv run pytest \
        test_streaming_class_e2e.py -v)
"""
import os
import pytest

requires_api_key = pytest.mark.skipif(
    not os.environ.get("OPENAI_API_KEY"),
    reason="streaming smoke needs OPENAI_API_KEY (use `infisical run --`)",
)

RESUME = (
    "Seasoned software engineer with 12 years of experience. Specializes "
    "in Python and Rust. Currently based in Berlin. Interests include "
    "distributed systems and developer tooling."
)


# ---------------------------------------------------------------------------
# Host-driven: the `S | StreamFinished` union and the final `T` both cross
# the FFI boundary. This is the path under investigation.
# ---------------------------------------------------------------------------


@requires_api_key
def test_class_stream_doc_next_reaches_finished():
    """`StreamingDoc` (3 fields) — drive `next()` to `StreamFinished`."""
    from baml_sdk.lorem import StreamE2EExtractDoc_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtractDoc_stream(RESUME)
    iterations = 0
    last_partial = None
    while True:
        v = stream.next()
        iterations += 1
        if isinstance(v, StreamFinished):
            break
        last_partial = v
        assert iterations < 10_000, "stream.next() failed to terminate"
    assert iterations >= 1
    # If we got any partial, it should be a StreamingDoc-shaped object.
    if last_partial is not None:
        assert hasattr(last_partial, "title") or isinstance(last_partial, dict), (
            f"unexpected partial type: {type(last_partial).__name__} = {last_partial!r}"
        )


@requires_api_key
def test_class_stream_doc_final_returns_complete_value():
    """`final()` returns a fully-typed `StreamingDoc`."""
    from baml_sdk.lorem import StreamE2EExtractDoc_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtractDoc_stream(RESUME)
    while not isinstance(stream.next(), StreamFinished):
        pass
    final = stream.final()

    assert final is not None
    # StreamingDoc has title / body / word_count.
    assert hasattr(final, "title"), f"expected StreamingDoc, got {final!r}"
    assert hasattr(final, "word_count"), f"expected StreamingDoc, got {final!r}"


@requires_api_key
def test_class_stream_resume_final_returns_complete_value():
    """Two-field class (`Resume`) sibling of the above."""
    from baml_sdk.lorem import StreamE2EExtractResume_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtractResume_stream(RESUME)
    while not isinstance(stream.next(), StreamFinished):
        pass
    final = stream.final()

    assert final is not None
    assert hasattr(final, "name"), f"expected Resume, got {final!r}"


@requires_api_key
async def test_class_stream_doc_async_reaches_finished():
    """Async sibling — exercises the pyo3 async bridge for a class `T`."""
    from baml_sdk.lorem import StreamE2EExtractDoc_stream_async
    from baml_sdk.baml.stream import StreamFinished

    stream = await StreamE2EExtractDoc_stream_async(RESUME)
    iterations = 0
    while True:
        v = await stream.next_async()
        iterations += 1
        if isinstance(v, StreamFinished):
            break
        assert iterations < 10_000, "stream.next_async() failed to terminate"
    final = await stream.final_async()
    assert final is not None
    assert hasattr(final, "title"), f"expected StreamingDoc, got {final!r}"


# ---------------------------------------------------------------------------
# BAML-driven: the `S | StreamFinished` union stays on the engine side; only
# the concrete final `T` (StreamingDoc) crosses the FFI boundary. Isolates
# the host bridge from the engine SAP path.
# ---------------------------------------------------------------------------


@requires_api_key
def test_class_stream_collect_in_baml():
    """`StreamE2ECollectDoc` loops `next()` inside BAML, returns `final()`.

    If this passes while the host-driven tests above fail, the break is
    in the host bridge's handling of `Stream<ClassT, ClassS>`, not the
    engine's SAP streaming path.
    """
    from baml_sdk.lorem import StreamE2ECollectDoc
    from baml_sdk.stream_types.lorem import StreamingDoc as StreamingDocPartial
    from baml_sdk.lorem import StreamingDoc

    result = StreamE2ECollectDoc(RESUME)

    assert result is not None
    assert isinstance(result, (StreamingDoc, StreamingDocPartial)), (
        f"expected a StreamingDoc, got {type(result).__name__} = {result!r}"
    )
    assert hasattr(result, "title")
