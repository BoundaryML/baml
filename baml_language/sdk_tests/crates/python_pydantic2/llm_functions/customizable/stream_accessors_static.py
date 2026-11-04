from typing_extensions import assert_type

from baml_sdk.ai.stream import Done
from baml_sdk.lorem import (
    StreamingDoc,
    StreamingExtract_spec,
    StreamingExtract_stream_sync,
    StreamingExtract_stream,
    stream_e2e_extract_stream_sync,
)
from baml_sdk.stream_typing import (
    TextResultStreamHolder,
    TextResultStreamMethods,
    accept_aliased_text_stream_sync,
    accept_text_stream_sync,
    call_text_stream_callback_sync,
    maybe_text_stream_sync,
    wrap_text_stream_sync,
)

stream = StreamingExtract_stream_sync("extract")
assert_type(stream.next(), StreamingDoc | Done)
assert_type(stream.final(), StreamingDoc)
assert_type(stream.next(_baml={"timeout_ms": 1000}), StreamingDoc | Done)
assert_type(stream.final(_baml={}), StreamingDoc)
stream.next(_baml={"unknown": True})  # type: ignore[typeddict-unknown-key]


async def check_async_accessors() -> None:
    async_stream = await StreamingExtract_stream("extract")
    assert_type(await async_stream.next_async(), StreamingDoc | Done)
    assert_type(await async_stream.final_async(), StreamingDoc)
    async for partial in async_stream:
        assert_type(partial, StreamingDoc)

    spec = await StreamingExtract_spec("extract")
    assert_type(await spec.name_async(_baml={}), str)
    spec.name(_baml={"unknown": True})  # type: ignore[typeddict-unknown-key]
    assert_type(
        await spec.parse_async('{"title":"x","body":"y","word_count":1}'), StreamingDoc
    )


text_stream = stream_e2e_extract_stream_sync("summarize")
assert_type(text_stream.next(), str | Done)
assert_type(text_stream.final(), str)

holder = TextResultStreamHolder(
    stream=text_stream,
    aliased=text_stream,
)
assert_type(holder.stream.next(), str | Done)
assert_type(holder.aliased.next(), str | Done)

assert_type(accept_text_stream_sync(text_stream).next(), str | Done)
assert_type(accept_aliased_text_stream_sync(text_stream).next(), str | Done)
assert_type(wrap_text_stream_sync(text_stream)[0].next(), str | Done)

maybe_stream = maybe_text_stream_sync(text_stream, True)
if maybe_stream is not None:
    assert_type(maybe_stream.next(), str | Done)

methods = TextResultStreamMethods()
assert_type(methods.echo_sync(text_stream).next(), str | Done)
assert_type(TextResultStreamMethods.echo_static_sync(text_stream).next(), str | Done)
assert_type(
    call_text_stream_callback_sync(lambda value: value, text_stream).next(),
    str | Done,
)
