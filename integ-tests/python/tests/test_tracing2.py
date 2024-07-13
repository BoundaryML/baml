import asyncio
import time
import pytest
from assertpy import assert_that
from dotenv import load_dotenv

load_dotenv()
import baml_py
from ..baml_client import b
from ..baml_client.globals import (
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
)
from ..baml_client.types import NamedArgsSingleEnumList, NamedArgsSingleClass
from ..baml_client.tracing import trace, set_tags, flush, on_log_event
from ..baml_client.type_builder import TypeBuilder
import datetime


@pytest.mark.asyncio
async def test_tracing_async_only():

    @trace
    async def top_level_async_tracing():
        @trace
        async def nested_dummy_fn(_foo: str):
            time.sleep(0.5 + random.random())
            return "nested dummy fn"

        @trace
        async def dummy_fn(foo: str):
            await asyncio.gather(
                b.FnOutputClass(foo),
                nested_dummy_fn(foo),
            )
            return "dummy fn"

        await asyncio.gather(
            dummy_fn("dummy arg 1"),
            dummy_fn("dummy arg 2"),
            dummy_fn("dummy arg 3"),
        )
        await asyncio.gather(
            parent_async("first-arg-value"), parent_async2("second-arg-value")
        )
        return 1

    # Clear any existing traces
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
    _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

    res = await top_level_async_tracing()
    assert_that(res).is_equal_to(1)

    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
    stats = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()
    print("STATS", stats)
    assert_that(stats.started).is_equal_to(15)
    assert_that(stats.finalized).is_equal_to(stats.started)
    assert_that(stats.submitted).is_equal_to(stats.started)
    assert_that(stats.sent).is_equal_to(stats.started)
    assert_that(stats.done).is_equal_to(stats.started)
    assert_that(stats.failed).is_equal_to(0)