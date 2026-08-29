"""HS-7 generated-SDK regressions for dynamic nominal stream identity."""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any

from replay_harness import replay_server

_EXPECTED_FINAL = {
    "title": (
        "Seasoned Python & Rust Software Engineer (Berlin) | "
        "Distributed Systems & Dev Tooling"
    ),
    "body": (
        "Seasoned software engineer with 12 years of experience, specializing "
        "in Python and Rust. Currently based in Berlin, with interests in "
        "distributed systems and developer tooling."
    ),
    "word_count": 26,
}


def _assert_repeated_pass_back(value: Any, echo: Callable[[Any], Any], expected: Any):
    from baml_bridge import BamlRuntimeValue

    first = echo(value)
    second = echo(first)
    assert isinstance(first, BamlRuntimeValue)
    assert isinstance(second, BamlRuntimeValue)
    assert first.to_data() == expected
    assert second.to_data() == expected


def _drain_runtime_stream(label: str, stream: Any) -> dict[str, Any]:
    from baml_bridge import BamlRuntimeValue
    from baml_sdk.ai.stream import Done
    from baml_sdk.lorem import Hs7Collision, hs7_echo_runtime_value

    non_null_partials = 0
    checked_pass_back = False
    next_calls = 0
    while True:
        partial = stream.next()
        if isinstance(partial, Done):
            break
        next_calls += 1
        assert next_calls < 10_000, "dynamic runtime stream failed to terminate"
        if partial is None:
            print(f"{label} partial[{next_calls}]=None")
            continue

        assert isinstance(partial, BamlRuntimeValue)
        assert not isinstance(partial, Hs7Collision)
        data = partial.to_data()
        print(f"{label} partial[{next_calls}]={partial!r} data={data!r}")
        assert isinstance(data, dict)
        non_null_partials += 1
        if not checked_pass_back:
            _assert_repeated_pass_back(partial, hs7_echo_runtime_value, data)
            checked_pass_back = True

    assert non_null_partials > 0
    final = stream.final()
    assert isinstance(final, BamlRuntimeValue)
    assert not isinstance(final, Hs7Collision)
    final_data = final.to_data()
    print(f"{label} final={final!r} data={final_data!r}")
    assert final_data == _EXPECTED_FINAL
    _assert_repeated_pass_back(final, hs7_echo_runtime_value, final_data)
    return final_data


async def _drain_runtime_stream_async_iterable(
    label: str, stream: Any
) -> dict[str, Any]:
    from baml_bridge import BamlRuntimeValue
    from baml_sdk.lorem import Hs7Collision, hs7_echo_runtime_value_async

    partial_count = 0
    checked_pass_back = False
    async for partial in stream:
        partial_count += 1
        assert isinstance(partial, BamlRuntimeValue)
        assert not isinstance(partial, Hs7Collision)
        data = partial.to_data()
        print(f"{label} async partial[{partial_count}]={partial!r} data={data!r}")
        assert isinstance(data, dict)

        if not checked_pass_back:
            first = await hs7_echo_runtime_value_async(partial)
            second = await hs7_echo_runtime_value_async(first)
            assert isinstance(first, BamlRuntimeValue)
            assert isinstance(second, BamlRuntimeValue)
            assert first.to_data() == data
            assert second.to_data() == data
            checked_pass_back = True

        assert partial_count < 10_000, "dynamic async stream failed to terminate"

    assert partial_count > 0
    final = await stream.final_async()
    assert isinstance(final, BamlRuntimeValue)
    assert not isinstance(final, Hs7Collision)
    final_data = final.to_data()
    print(f"{label} async final={final!r} data={final_data!r}")
    assert final_data == _EXPECTED_FINAL
    return final_data


# SDK_PARITY_LINT(skip): validates Python BamlRuntimeValue identity through the generated sync stream surface
@replay_server(recording_path="replay_extract_doc")
def test_dynamic_runtime_stream_identity_and_flat_projection_parity():
    from baml_bridge import BamlFunctionSpec, BamlRuntimeValue
    from baml_sdk.lorem import (
        Hs7Collision,
        StreamingDoc,
        hs7_dynamic_extract_stream,
        hs7_echo_runtime_value,
        hs7_open_collision_spec,
        hs7_open_collision_stream,
        hs7_open_collision_value,
    )

    # First mirror the original VetRec wrapper: the runtime type is constructed
    # inside BAML and only the public Stream slots are widened to `unknown`.
    wrapped_final = _drain_runtime_stream(
        "wrapped", hs7_open_collision_stream("ignored-by-replay-server")
    )

    # Receive the same BAML-created spec as a handle through an `unknown`
    # declaration. Its realized type argument, not its display name, must
    # drive parse and direct-result decoding.
    dynamic_spec = hs7_open_collision_spec("ignored-by-replay-server")
    assert isinstance(dynamic_spec, BamlFunctionSpec)

    # Direct results and bound-spec parsing must make the same
    # dynamic-vs-compiled identity decision as stream next/final. The direct
    # value is decoded from JSON inside BAML so this identity check stays
    # independent of the fixture's intentionally SSE-only replay transport.
    raw_final = json.dumps(_EXPECTED_FINAL)
    dynamic_results = {
        "direct": hs7_open_collision_value(raw_final),
        "spec-parse": dynamic_spec.parse(raw_final),
    }
    for label, value in dynamic_results.items():
        assert isinstance(value, BamlRuntimeValue)
        assert not isinstance(value, Hs7Collision)
        data = value.to_data()
        print(f"{label}={value!r} data={data!r}")
        assert data == _EXPECTED_FINAL
        _assert_repeated_pass_back(value, hs7_echo_runtime_value, data)

    # The flat generated shortcut selects the compiler-private `Fn@stream`
    # entry. A compiled token keeps this parity pin independent of Python
    # reflection while the BAML wrapper above covers live dynamic identity.
    flat_stream = hs7_dynamic_extract_stream(
        "ignored-by-replay-server", _types={"T": StreamingDoc}
    )
    flat_final = flat_stream.final()
    assert isinstance(flat_final, StreamingDoc)
    flat_data = flat_final.model_dump()
    print(f"flat final={flat_final!r} data={flat_data!r}")

    assert wrapped_final == flat_data == _EXPECTED_FINAL


# SDK_PARITY_LINT(skip): validates Python's generated async-iterable stream surface
@replay_server(recording_path="replay_extract_doc")
async def test_dynamic_runtime_stream_is_an_elegant_async_iterable():
    from baml_sdk.lorem import hs7_open_collision_stream_async

    # The runtime output type is built and bound entirely inside BAML. Python
    # receives an ordinary async iterable of opaque, identity-preserving values.
    stream = await hs7_open_collision_stream_async("ignored-by-replay-server")
    final_data = await _drain_runtime_stream_async_iterable("async-for", stream)
    assert final_data == _EXPECTED_FINAL
