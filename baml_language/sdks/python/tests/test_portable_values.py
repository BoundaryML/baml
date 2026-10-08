from __future__ import annotations

from typing import Any

import pydantic

from baml_bridge.baml_py import BamlImage, _encode_inbound
from baml_bridge.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2
from baml_bridge.proto import decode_value
from baml_bridge.typemap import BamlTypeMap, get_type_map, set_type_map


class _Prompt(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(populate_by_name=True)
    field_data: Any = pydantic.Field(alias="_data")


def _prompt_typemap() -> BamlTypeMap:
    return BamlTypeMap.from_lazy_entries(
        classes={"ai.Prompt": (_Prompt.__module__, _Prompt.__qualname__)},
        enums={},
        type_aliases={},
    )


def test_media_decodes_and_reencodes_as_portable_payload():
    outbound = baml_outbound_pb2.BamlOutboundValue()
    outbound.media_value.media = baml_outbound_pb2.IMAGE
    outbound.media_value.mime_type = "image/png"
    outbound.media_value.base64 = "aW1hZ2U="

    media = decode_value(outbound, BamlTypeMap())
    assert isinstance(media, BamlImage)
    assert media.base64() == "aW1hZ2U="
    assert media.mime_type() == "image/png"

    inbound = baml_inbound_pb2.InboundValue.FromString(_encode_inbound(media, "media"))
    assert inbound.WhichOneof("value") == "media_value"
    assert inbound.media_value.base64 == "aW1hZ2U="
    assert inbound.media_value.mime_type == "image/png"


def test_prompt_wrapper_reencodes_repeatedly_without_a_handle():
    outbound = baml_outbound_pb2.BamlOutboundValue()
    message = outbound.prompt_ast_value.message
    message.role = "user"
    message.metadata_as_json = "{}"
    message.content.multiple.items.add().string = "look: "
    media = message.content.multiple.items.add().media
    media.media = baml_outbound_pb2.IMAGE
    media.mime_type = "image/png"
    media.base64 = "aW1hZ2U="

    typemap = _prompt_typemap()
    prompt = decode_value(outbound, typemap)
    assert isinstance(prompt, _Prompt)

    saved = get_type_map()
    set_type_map(typemap)
    try:
        for _ in range(2):
            inbound = baml_inbound_pb2.InboundValue.FromString(
                _encode_inbound(prompt, "prompt")
            )
            assert inbound.WhichOneof("value") == "prompt_ast_value"
            assert inbound.prompt_ast_value.message.role == "user"
            assert (
                inbound.prompt_ast_value.message.content.multiple.items[1].media.base64
                == "aW1hZ2U="
            )
    finally:
        set_type_map(saved)
