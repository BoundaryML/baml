"""Keyless child process used by the stream recording regression."""

import asyncio
import sys

from baml_bridge import BamlError, shutdown_runtime
from baml_sdk import trace
from baml_sdk.lorem import (
    trace_stream_doc_stream_async,
    trace_stream_string_stream_async,
)
from replay_harness import replay_server


@replay_server(recording_path="replay_extract_string")
async def record(marker):
    def options(case):
        return {
            "trace": trace.context(
                metadata={"stream_audit": marker, "case": case}, distinct_id="u1"
            )
        }

    @trace.instrument
    async def create():
        return await trace_stream_string_stream_async("Ada", _baml=options("escaped"))

    stream = await create()
    async for _ in stream:
        pass
    await stream.final_async()

    # No host parent, context-only options: one function name, one tree.
    root = await trace_stream_string_stream_async("Ada", _baml=options("root"))
    await root.final_async()

    @trace.instrument
    async def together():
        first = await trace_stream_string_stream_async(
            "Ada", _baml=options("concurrent-string")
        )
        second = await trace_stream_doc_stream_async(
            "Ada", _baml=options("concurrent-doc")
        )
        await asyncio.gather(
            first.final_async(), second.final_async(), return_exceptions=True
        )

    await together()

    # Abandoning a handle completes its span as cancelled, without executing
    # an HTTP request or holding runtime shutdown open.
    abandoned = await trace_stream_string_stream_async(
        "Ada", _baml=options("abandoned")
    )
    del abandoned

    # Keep EOF's parse failure on the origin span, and still report it from
    # final() after the sentinel has been delivered.
    failure = await trace_stream_doc_stream_async("Ada", _baml=options("parse-error"))
    try:
        async for _ in failure:
            pass
        await failure.final_async()
    except BamlError:
        pass


if __name__ == "__main__":
    asyncio.run(record(sys.argv[1]))
    shutdown_runtime()
