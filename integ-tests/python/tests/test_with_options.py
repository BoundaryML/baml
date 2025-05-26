import pytest

from ..baml_client import b
from ..baml_client.sync_client import b as b_sync
from baml_py import Collector
import gc
import sys


@pytest.fixture(autouse=True)
def ensure_collector_is_empty():
    assert Collector.__function_call_count() == 0
    yield
    gc.collect()
    assert Collector.__function_call_count() == 0


@pytest.mark.asyncio
async def test_with_options_logger_async():
    print("### function_call_count", Collector.__function_call_count())
    # garbage collected!
    assert Collector.__function_call_count() == 0

    collector = Collector(name="my-collector")
    function_logs = collector.logs
    # print("### function_logs", function_logs, file=sys.stderr)
    assert len(function_logs) == 0

    my_b = b.with_options(collector=collector)

    await my_b.TestOpenAIGPT4oMini("hi there")

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "call"

    # Verify usage fields
    assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert len(calls) == 1

    await b.TestOpenAIGPT4oMini("hi there")
    assert (
        len(collector.logs) == 1
    )  # didn't get logged on original client since it's not passed in options

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert Collector.__function_call_count() > 0


def test_with_options_logger_sync():
    collector = Collector(name="my-collector")
    my_b = b_sync.with_options(collector=collector)
    my_b.TestOpenAIGPT4oMini("hi there")
    assert len(collector.logs) == 1


@pytest.mark.asyncio
async def test_with_options_logger_async_stream():
    collector = Collector(name="my-collector")
    my_b = b.with_options(collector=collector)
    assert len(collector.logs) == 0
    stream = my_b.stream.TestOpenAIGPT4oMini("hi there")
    async for chunk in stream:
        pass
    assert len(collector.logs) == 1
