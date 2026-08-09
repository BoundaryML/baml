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
