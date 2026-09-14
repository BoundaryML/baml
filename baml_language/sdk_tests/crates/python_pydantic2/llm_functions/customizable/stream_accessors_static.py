from typing import assert_type

from baml_sdk.ai.stream import Done
from baml_sdk.lorem import (
    StreamingDoc,
    StreamingExtract_spec_async,
    StreamingExtract_stream,
    StreamingExtract_stream_async,
    stream_e2e_extract_stream,
)
from baml_sdk.stream_typing import (
    TextResultStreamHolder,
    TextResultStreamMethods,
    accept_aliased_text_stream,
    accept_text_stream,
    call_text_stream_callback,
    maybe_text_stream,
    wrap_text_stream,
)
from baml_sdk.stream_types.ai.stream import Stream as PartialStreamState
from baml_sdk.stream_types.lorem import StreamingDoc as PartialStreamingDoc
from baml_sdk.stream_types.stream_typing import (
    TextResultStream as PartialTextResultStream,
    TextResultStreamHolder as PartialTextResultStreamHolder,
)


stream = StreamingExtract_stream("extract")
assert_type(stream.next(), PartialStreamingDoc | None | Done)
assert_type(stream.final(), StreamingDoc)


async def check_async_accessors() -> None:
    async_stream = await StreamingExtract_stream_async("extract")
    assert_type(await async_stream.next_async(), PartialStreamingDoc | None | Done)
    assert_type(await async_stream.final_async(), StreamingDoc)
    async for partial in async_stream:
        assert_type(partial, PartialStreamingDoc)

    spec = await StreamingExtract_spec_async("extract")
    assert_type(
        await spec.parse_async('{"title":"x","body":"y","word_count":1}'), StreamingDoc
    )


text_stream = stream_e2e_extract_stream("summarize")
assert_type(text_stream.next(), str | None | Done)
assert_type(text_stream.final(), str)

holder = TextResultStreamHolder(
    stream=text_stream,
    aliased=text_stream,
    completed_stream=text_stream,
)
assert_type(holder.stream.next(), str | None | Done)
assert_type(holder.aliased.next(), str | None | Done)
assert_type(holder.completed_stream.next(), str | None | Done)

assert_type(accept_text_stream(text_stream).next(), str | None | Done)
assert_type(accept_aliased_text_stream(text_stream).next(), str | None | Done)
assert_type(wrap_text_stream(text_stream)[0].next(), str | None | Done)

maybe_stream = maybe_text_stream(text_stream, True)
if maybe_stream is not None:
    assert_type(maybe_stream.next(), str | None | Done)

methods = TextResultStreamMethods()
assert_type(methods.echo(text_stream).next(), str | None | Done)
assert_type(TextResultStreamMethods.echo_static(text_stream).next(), str | None | Done)
assert_type(
    call_text_stream_callback(lambda value: value, text_stream).next(),
    str | None | Done,
)


def check_partial_stream_positions(
    holder: PartialTextResultStreamHolder,
    aliased: PartialTextResultStream,
) -> None:
    assert_type(holder.stream, PartialStreamState[str | None, str] | None)
    assert_type(holder.aliased, PartialTextResultStream | None)
    assert_type(holder.completed_stream.next(), str | None | Done)
    assert_type(aliased, PartialTextResultStream)
