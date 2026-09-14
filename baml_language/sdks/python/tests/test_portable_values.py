from __future__ import annotations

from typing import Any

import pydantic

from baml_bridge.baml_py import BamlImage
from baml_bridge.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2
from baml_bridge.proto import _set_inbound_value, decode_value
from baml_bridge.typemap import BamlTypeMap


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

    inbound = baml_inbound_pb2.InboundValue()
    _set_inbound_value(inbound, media, kwarg_name="media")
    assert inbound.WhichOneof("value") == "media_value"
    assert inbound.media_value.base64 == "aW1hZ2U="


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

    prompt = decode_value(outbound, _prompt_typemap())
    assert isinstance(prompt, _Prompt)

    for _ in range(2):
        inbound = baml_inbound_pb2.InboundValue()
        _set_inbound_value(inbound, prompt, kwarg_name="prompt")
        assert inbound.WhichOneof("value") == "prompt_ast_value"
        assert inbound.prompt_ast_value.message.role == "user"
        assert (
            inbound.prompt_ast_value.message.content.multiple.items[1].media.base64
            == "aW1hZ2U="
        )
