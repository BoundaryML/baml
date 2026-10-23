"""Host-supplied json must materialize with `json` container typing.

Inbound dicts/lists from the Python bridge carry no element-type
annotation on the wire; the engine must re-annotate them with the
`baml.json.json` alias so typed narrowing inside BAML — `match (j) {
let m: map<string, json> => ... }`, and therefore `baml.json.path` /
`path_or` — treats them exactly like BAML-born `baml.json.parse`
values.
"""

from __future__ import annotations

import pytest


# SDK_PARITY_LINT(skip): C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests)
def test_host_supplied_json_supports_typed_narrowing():
    from baml_sdk.baml import BamlError
    from baml_sdk.go_json_tests import json_kind, json_path_string, json_path_string_or

    obj = {
        "type": "ok",
        "nested": {"list": [1, {"deep": "found"}]},
    }

    assert json_kind(obj) == "object"
    assert json_kind([1]) == "array"
    assert json_kind("text") == "string"
    assert json_kind(3) == "other"

    assert json_path_string(obj, ".type") == "ok"
    assert json_path_string(obj, ".nested.list[1].deep") == "found"
    assert json_path_string_or(obj, ".missing", "fallback") == "fallback"

    with pytest.raises(BamlError, match="missing field"):
        json_path_string(obj, ".absent")


# SDK_PARITY_LINT(skip): C# declares no function_calls suite (its native coverage is Rust-wrapped integration tests)
def test_json_returned_from_host_callback_supports_typed_narrowing():
    from baml_sdk.go_json_tests import json_callback_kind

    # json returned from a host callback converts on the host-return path
    # (no argument coercion pass); it must narrow identically.
    assert json_callback_kind(lambda v: {"wrapped": v}, "payload") == "object"


def _nested_json(depth: int) -> object:
    """A json value whose lists nest `depth` deep around a `1`."""
    value: object = 1
    for _ in range(depth):
        value = [value]
    return value


def test_json_nested_deeper_than_the_wire_carries_is_refused_as_an_argument():
    from baml_sdk.go_json_tests import round_trip_json

    # Well inside the bound, the value crosses both ways.
    assert round_trip_json(_nested_json(30)) == _nested_json(30)
    # Just past it, and far past it: refused before anything is sent, with
    # the error an argument that cannot be encoded raises, never by
    # exhausting the stack.
    for depth in (60, 1000):
        with pytest.raises(TypeError, match="nested too deeply"):
            round_trip_json(_nested_json(depth))


def test_json_nested_deeper_than_the_wire_carries_is_refused_as_a_callback_result():
    from baml_sdk.baml import BamlPanic
    from baml_sdk.baml.panics import SdkPanic
    from baml_sdk.go_json_tests import call_json_callback

    # Well inside the bound, the callback's result crosses.
    assert call_json_callback(lambda _: _nested_json(30), 1) == _nested_json(30)
    # Past it, the bridge cannot carry the result: it is refused before it is
    # sent, and the call raises the engine's panic for a bridge failure.
    for depth in (60, 200):
        with pytest.raises(BamlPanic) as exc_info:
            call_json_callback(lambda _, depth=depth: _nested_json(depth), 1)
        panic = exc_info.value.value
        assert isinstance(panic, SdkPanic)
        assert "nested too deeply" in panic.message
