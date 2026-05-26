"""End-to-end streaming smoke against OpenAI for the
`baml.llm.Stream` → `BamlStream(BamlPyHandle)` rewrite (21a, 21b).

Returns `string` (so the synthesized `$stream` companion is
`Stream<string, null | string>` — the only streaming shape the BAML
engine accepts today; class-typed `T` hits `Non-parsable type: Void`
at `StreamCache.new`, tracked as a separate engine task in 21b §"Engine
constraint").

This is the red baseline for phase 0 of the plan. It exercises the
`Stream<T, S>` API today (four-field Pydantic shell route) so the
failure mode is captured in commit history; once phases 1-5 land, the
same module flips green without further edits.

Skipped unless OPENAI_API_KEY is set. Run via:

    infisical run -- cargo test -p sdk_test_llm_functions pytest

The 100-input parametrize case is gated behind the BAML_STREAM_E2E_FULL
env var (≈5min wall clock, ≈$0.50/run against gpt-4o-mini), so default
CI runs the two single-shot smokes and the async smoke only.
"""
import os
import pytest

requires_api_key = pytest.mark.skipif(
    not os.environ.get("OPENAI_API_KEY"),
    reason="streaming smoke needs OPENAI_API_KEY (use `infisical run --`)",
)


# ---------------------------------------------------------------------------
# 100 distinct ~50-word resume blurbs. Generated programmatically so the
# file stays small; each varies role, years, skills, and an adjective so
# the model produces visibly different summaries.
# ---------------------------------------------------------------------------


def _generate_resumes() -> list[str]:
    roles = [
        "software engineer", "data scientist", "product manager",
        "designer", "DevOps engineer", "ML researcher",
        "frontend developer", "backend developer", "SRE",
        "security engineer",
    ]
    adjectives = [
        "seasoned", "junior", "principal", "staff", "lead", "senior",
        "associate", "consulting", "freelance", "tenured",
    ]
    skills = [
        "Python and Rust", "Go and Kubernetes", "TypeScript and React",
        "SQL and Snowflake", "PyTorch and CUDA", "Terraform and AWS",
        "Swift and SwiftUI", "Kotlin and Android",
        "C++ and embedded systems", "Elixir and Phoenix",
    ]
    out = []
    for i in range(100):
        role = roles[i % len(roles)]
        adj = adjectives[(i // len(roles)) % len(adjectives)]
        primary = skills[(i * 7) % len(skills)]
        secondary = skills[(i * 3) % len(skills)]
        years = 2 + (i % 18)
        out.append(
            f"{adj.capitalize()} {role} with {years} years of "
            f"experience. Specializes in {primary}. Currently based in "
            f"city #{i + 1}. Interests include {secondary}."
        )
    assert len(out) == 100
    assert len(set(out)) == 100, "resumes must be distinct"
    return out


RESUMES = _generate_resumes()


# ---------------------------------------------------------------------------
# Single-shot smokes (always run when OPENAI_API_KEY is set).
# ---------------------------------------------------------------------------


@requires_api_key
def test_stream_next_reaches_finished():
    """`next()` drives the stream to completion via the bridge path.

    Calling `stream.next()` repeatedly must terminate with a
    `StreamFinished` instance — proving the lifted `Adt(Stream(handle))`
    round-trips through `bex.call_function("baml.llm.Stream.next", ...)`
    without crashing or leaking handles.

    Note: with `S = null | string`, the SAP partial parser typically
    returns `StreamNoYield` for plain-text content until the stream is
    done — so `Stream.next()`'s inner loop drains all SSE events and
    returns `StreamFinished` on the first call. That's a streaming-
    parser concern (engine-side), not a bridge concern; this test only
    pins the bridge contract.
    """
    from baml_sdk.lorem import StreamE2EExtract_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtract_stream(RESUMES[0])
    iterations = 0
    while True:
        v = stream.next()
        iterations += 1
        if isinstance(v, StreamFinished):
            break
        # Accept either None (in-progress, S = null) or string partials.
        assert v is None or isinstance(v, str), (
            f"unexpected stream.next() return type: {type(v).__name__}"
        )
        assert iterations < 10_000, "stream.next() failed to terminate"
    assert iterations >= 1


@requires_api_key
def test_stream_final_returns_complete_value():
    """`final()` after exhausting `next()` returns the full string."""
    from baml_sdk.lorem import StreamE2EExtract_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtract_stream(RESUMES[1])
    while not isinstance(stream.next(), StreamFinished):
        pass
    final = stream.final()

    assert isinstance(final, str)
    assert len(final) > 50, f"expected a multi-line summary; got {final!r}"
    # Sanity: the bullet-point format should make this true for
    # anything but a degenerate response.
    assert "-" in final or "\n" in final, (
        f"expected bullet/newline structure; got: {final!r}"
    )


@requires_api_key
def test_stream_collect_in_baml():
    """BAML-driven counterpart to `test_stream_next_reaches_finished`.

    `StreamE2ECollect` calls `stream.next()` in a BAML `while` loop,
    collecting every non-`StreamFinished` yield into an `(null |
    string)[]`, then calls `stream.final()` once. Returns the aggregate
    as `StreamE2ECollectResult { next_calls, final_call }` — a normal
    user class with concrete field types.

    Where this differs from the host-driven tests: the `S |
    StreamFinished` union never crosses the FFI boundary. Only the
    aggregate (concrete types, no `Ty::TypeVar`) does. If the
    aggregate decodes cleanly while the host-driven tests still error
    on `find_matching_member`, the failure is conclusively in
    `tir2_to_template`'s handling of `Stream.next`'s declared return
    type, not in the bridge / SSE / parser path.
    """
    from baml_sdk.lorem import StreamE2ECollect, StreamE2ECollectResult

    result = StreamE2ECollect(RESUMES[3])

    assert isinstance(result, StreamE2ECollectResult)
    assert isinstance(result.next_calls, list)
    assert len(result.next_calls) >= 1, (
        f"expected at least one yield before StreamFinished, got {result.next_calls!r}"
    )
    for i, item in enumerate(result.next_calls):
        assert item is None or isinstance(item, str), (
            f"next_calls[{i}] has unexpected type {type(item).__name__}: {item!r}"
        )

    assert isinstance(result.final_call, str)
    assert len(result.final_call) > 50, (
        f"expected a multi-line summary; got {result.final_call!r}"
    )
    assert "-" in result.final_call or "\n" in result.final_call, (
        f"expected bullet/newline structure; got: {result.final_call!r}"
    )


@requires_api_key
async def test_stream_async_reaches_finished():
    """Async sibling of `test_stream_next_reaches_finished`.

    Exercises the `pyo3_async_runtimes::tokio::future_into_py` path so
    we know the async bridge round-trip works. Same caveat as the sync
    test: parser-yielding semantics for `null | string` partials are an
    engine concern; this only pins the bridge contract.

    `pytest-asyncio` with `asyncio_mode = "auto"` (set in build.rs)
    runs `async def test_*` without an explicit decorator.
    """
    from baml_sdk.lorem import StreamE2EExtract_stream_async
    from baml_sdk.baml.stream import StreamFinished

    stream = await StreamE2EExtract_stream_async(RESUMES[2])
    iterations = 0
    while True:
        v = await stream.next_async()
        iterations += 1
        if isinstance(v, StreamFinished):
            break
        assert v is None or isinstance(v, str), (
            f"unexpected stream.next_async() return type: {type(v).__name__}"
        )
        assert iterations < 10_000, "stream.next_async() failed to terminate"
    assert iterations >= 1


# ---------------------------------------------------------------------------
# Heavy fan-out — 100 distinct inputs. ~5min wall clock at $0.50/run on
# gpt-4o-mini. Gated behind BAML_STREAM_E2E_FULL=1 so default CI runs
# only the single-shot smokes.
# ---------------------------------------------------------------------------


full_run_only = pytest.mark.skipif(
    not os.environ.get("BAML_STREAM_E2E_FULL"),
    reason="set BAML_STREAM_E2E_FULL=1 to run the 100-input fan-out",
)


@requires_api_key
@full_run_only
@pytest.mark.parametrize("idx", range(100))
def test_stream_100_distinct_inputs(idx: int):
    """100 distinct inputs × 100 distinct outputs.

    Each call must:
      (a) drain `next()` to `StreamFinished` without crashing
          (proves the bridge round-trip is robust across inputs),
      (b) produce a non-empty `final()` string (proves the typed decode
          path round-tripped end-to-end).

    Parameterizing surfaces input-specific bridge bugs a single-shot
    smoke would miss. The intermediate-partial yield is engine
    parser-side and not asserted here — see
    `test_stream_next_reaches_finished` for the rationale.
    """
    from baml_sdk.lorem import StreamE2EExtract_stream
    from baml_sdk.baml.stream import StreamFinished

    stream = StreamE2EExtract_stream(RESUMES[idx])
    while not isinstance(stream.next(), StreamFinished):
        pass

    final = stream.final()
    assert isinstance(final, str)
    assert len(final) > 20, f"resume #{idx} produced trivial final: {final!r}"
