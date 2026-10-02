import asyncio
import json

import pytest

import baml_sdk
from baml_sdk import lorem
from baml_sdk.baml.media import Image

PNG_B64 = "iVBORw0KGgo="


def _walk_dicts(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk_dicts(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_dicts(child)


# SDK_PARITY_LINT(skip): validates Python's portable Prompt and media wrapper surface
def test_prompt_is_reusable_and_media_survives_request_preview():
    image = Image.from_base64(PNG_B64, "image/png")
    spec = lorem.InspectMedia_spec(photo=image)
    prompt = spec.prompt()

    first_text = prompt.text()
    assert prompt.text() == first_text

    first_messages = prompt.messages()
    assert prompt.text() == first_text
    second_messages = prompt.messages()
    assert len(first_messages) == len(second_messages) == 1
    assert first_messages[0].role == second_messages[0].role == "user"
    assert first_messages[0].parts[0] == second_messages[0].parts[0]
    assert first_messages[0].parts[0].startswith("Describe this image:")

    media_parts = [
        part for part in first_messages[0].parts if not isinstance(part, str)
    ]
    assert len(media_parts) == 1
    assert media_parts[0].base64() == PNG_B64
    assert media_parts[0].mime_type() == "image/png"

    # Rendering another portable Prompt from the same live spec must not
    # consume either the spec or the first Prompt's owned AST.
    second_prompt = spec.prompt()
    assert second_prompt.text() == first_text
    assert second_prompt.messages()[0].parts[1].base64() == PNG_B64

    request = spec.build_request()
    body = json.loads(request.body)
    image_parts = [
        part for part in _walk_dicts(body) if part.get("type") == "input_image"
    ]
    assert len(image_parts) == 1
    assert image_parts[0]["image_url"] == "data:image/png;base64," + PNG_B64

    assert spec.build_request().body == request.body


# SDK_PARITY_LINT(skip): validates Python's generated FunctionSpec parse surface
def test_function_spec_parse_replaces_the_parse_companion():
    spec = lorem.ExtractResume_spec(text="Ada Lovelace")
    parsed = spec.parse('{"name":"Ada Lovelace","email":null}')

    assert isinstance(parsed, lorem.Resume)
    assert parsed.name == "Ada Lovelace"
    assert parsed.email is None


def test_function_spec_helpers_accept_invocation_controls():
    from baml_bridge import BamlCancelledError

    spec = lorem.ExtractResume_spec(text="Ada Lovelace", _baml={})
    assert spec.name(_baml={"timeout_ms": 1000})
    for method in ("name", "arguments", "output_type", "prompt", "tools", "client_id", "build_request", "parse", "call"):
        args = ('{"name":"Ada","email":null}',) if method == "parse" else ()
        with pytest.raises(BamlCancelledError):
            getattr(spec, method)(*args, _baml={"timeout_ms": 0})


async def test_function_spec_helpers_accept_invocation_controls_async():
    spec = await lorem.ExtractResume_spec_async(text="Ada Lovelace", _baml={})
    assert await spec.name_async(_baml={"timeout_ms": 1000})
    for method in ("name", "arguments", "output_type", "prompt", "tools", "client_id", "build_request", "parse", "call"):
        args = ('{"name":"Ada","email":null}',) if method == "parse" else ()
        with pytest.raises(asyncio.CancelledError):
            await getattr(spec, f"{method}_async")(*args, _baml={"timeout_ms": 0})
