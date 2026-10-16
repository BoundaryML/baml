"""An optional field the caller never set crosses into BAML absent, not null.

Pydantic records which fields a model was given (`model_fields_set`). The
bridge leaves out a field that is not in that set and still holds its `None`
default, so BAML sees it absent; an explicit `None` is sent as `null`.
"""

from __future__ import annotations

from typing import Optional

import pydantic

from baml_bridge import BamlRuntime, call_function_sync
from baml_bridge.cffi.v1 import baml_inbound_pb2
from baml_bridge.proto import _set_inbound_value
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


class Patch(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="ignore", populate_by_name=True)

    name: Optional[str] = None
    expires_in: Optional[int] = None


PATCH_BAML = """\
class Patch {
    name: string?,
    expires_in: int?,
}

function Encode(p: Patch) -> string {
    baml.json.to_string(p, omit_absent = true)
}

function EncodeDefault(p: Patch) -> string {
    baml.json.to_string(p)
}
"""


def _wire_fields(model: pydantic.BaseModel) -> dict[str, str | None]:
    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, model, kwarg_name="p")
    assert inbound.WhichOneof("value") == "class_value"
    return {
        entry.string_key: entry.value.WhichOneof("value")
        for entry in inbound.class_value.fields
    }


def test_unset_field_is_left_out_and_explicit_none_is_sent():
    # `None` for the oneof means the entry is present and null.
    assert _wire_fields(Patch(name="x")) == {"name": "string_value"}
    assert _wire_fields(Patch(name="x", expires_in=None)) == {
        "name": "string_value",
        "expires_in": None,
    }
    assert _wire_fields(Patch.model_validate({"expires_in": None})) == {
        "expires_in": None
    }
    assert _wire_fields(Patch()) == {}


def test_assigning_a_field_sets_it():
    patch = Patch(name="x")
    patch.expires_in = None
    assert _wire_fields(patch) == {"name": "string_value", "expires_in": None}


def test_presence_reaches_baml_json_encoding():
    runtime = BamlRuntime.initialize_runtime(".", {"main.baml": PATCH_BAML})
    saved = get_type_map()
    set_type_map(
        BamlTypeMap.from_lazy_entries(
            classes={"user.Patch": (Patch.__module__, Patch.__name__)},
            enums={},
            type_aliases={},
        )
    )

    def encode(function: str, patch: Patch) -> str:
        return call_function_sync(runtime, function, {"p": patch}).result()

    try:
        assert encode("Encode", Patch(name="x")) == '{"name":"x"}'
        assert encode("Encode", Patch(expires_in=None)) == '{"expires_in":null}'
        assert encode("Encode", Patch()) == "{}"
        # Without `omit_absent`, BAML writes an absent field as null, as before.
        assert (
            encode("EncodeDefault", Patch(name="x")) == '{"name":"x","expires_in":null}'
        )
    finally:
        set_type_map(saved)
