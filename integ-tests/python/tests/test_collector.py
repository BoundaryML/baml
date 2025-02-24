import uuid
import json
import os
import time
from typing import List, Optional
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64

# warning -- using this object -- even just instantiating it with dummy values, makes it so some
# of our pyo3 objects dont get cleaned up until after pytest starts exiting.
from openai.types.chat import ChatCompletion

load_dotenv()
import baml_py
from baml_py import errors

from ..baml_client import b
from ..baml_client.sync_client import b as b_sync
from ..baml_client.tracing import trace
from baml_py import Collector, FunctionLog
import gc
import sys


@pytest.fixture(autouse=True)
def ensure_collector_is_empty():
    assert Collector.__function_span_count() == 0
    yield
    gc.collect()
    assert Collector.__function_span_count() == 0


@pytest.mark.asyncio
async def test_collector_async_no_stream_success():
    print("### function_span_count", Collector.__function_span_count())
    # garbage collected!
    assert Collector.__function_span_count() == 0

    collector = Collector(name="my-collector")
    function_logs = collector.logs
    # print("### function_logs", function_logs, file=sys.stderr)
    assert len(function_logs) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})

    function_logs = collector.logs
    # print("### function_logs2", function_logs, file=sys.stderr)
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "call"

    # Verify timing fields
    assert log.timing.start_time_utc_ms > 0
    assert log.timing.duration_ms is not None and log.timing.duration_ms > 0

    # TODO: add this api
    # assert log.timing.time_to_first_parsed_ms is not None
    # assert log.timing.time_to_first_parsed_ms > 0

    # Verify usage fields
    assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert len(calls) == 1

    call = calls[0]

    assert call.provider == "openai"
    assert call.client_name == "GPT4oMini"
    assert call.selected

    # Verify request/response
    request = call.http_request
    assert request is not None
    print(f"### request.body: {request.body} \n {type(request.body)}", file=sys.stderr)
    assert isinstance(request.body, dict)
    assert "messages" in request.body
    assert "content" in request.body["messages"][0]
    assert request.body["messages"][0]["content"] is not None
    assert request.body["model"] == "gpt-4o-mini"

    # Verify http response
    response = call.http_response
    assert response is not None
    assert response.status == 200
    assert response.body is not None
    assert isinstance(response.body, dict)
    completion = ChatCompletion(**response.body)
    assert "choices" in response.body
    assert len(response.body["choices"]) > 0
    assert "message" in response.body["choices"][0]
    assert "content" in response.body["choices"][0]["message"]
    assert completion.choices[0].message.content is not None

    # Verify call timing
    call_timing = call.timing
    assert call_timing.start_time_utc_ms > 0
    assert call_timing.duration_ms is not None and call_timing.duration_ms > 0

    # Verify call usage
    call_usage = call.usage
    assert call_usage.input_tokens is not None and call_usage.input_tokens > 0
    assert call_usage.output_tokens is not None and call_usage.output_tokens > 0
    # it matches the log usage
    assert call_usage.input_tokens == log.usage.input_tokens
    assert call_usage.output_tokens == log.usage.output_tokens

    # Verify raw response exists
    assert log.raw_llm_response is not None

    assert collector.usage.input_tokens == log.usage.input_tokens
    assert collector.usage.output_tokens == log.usage.output_tokens

    # Verify metadata
    assert isinstance(log.metadata, dict)

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert Collector.__function_span_count() > 0


@pytest.mark.asyncio
async def test_collector_async_no_stream_no_getting_logs():
    collector = Collector(name="my-collector")
    function_logs = collector.logs
    assert len(function_logs) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})
    # async for chunk in stream:
    #     print(f"### chunk: {chunk}")

    # TODO: possible bug -- if no functionLog pyo3 objects are created, that function ref count is always 1
    # and it never goes away.
    # function_logs = collector.logs

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert Collector.__function_span_count() > 0


@pytest.mark.asyncio
async def test_collector_async_stream_success():
    collector = Collector(name="my-collector")
    function_logs = collector.logs
    assert len(function_logs) == 0

    stream = b.stream.TestOpenAIGPT4oMini(
        "hi there", baml_options={"collector": collector}
    )

    async for chunk in stream:
        print(f"### chunk: {chunk}")

    res = await stream.get_final_response()
    print(f"### res: {res}")
    # TODO: possible bug -- if no functionLog pyo3 objects are created, that function ref count is always 1
    # and it never goes away.
    # TODO: is FunctionResultStream (pyo3 class) never getting finalized? somethings up with that no? That one has the collector
    function_logs = collector.logs

    # function_logs = collector.logs
    # assert len(function_logs) == 1

    # log = collector.last
    # assert log is not None
    # assert log.function_name == "TestOpenAIGPT4oMini"
    # assert log.log_type == "call"

    # function_logs = collector.logs
    # assert len(function_logs) == 1

    # log = collector.last
    # assert log is not None
    # assert log.function_name == "TestOpenAIGPT4oMini"
    # assert log.log_type == "call"

    # # Verify timing fields
    # assert log.timing.start_time_utc_ms > 0
    # assert log.timing.duration_ms is not None and log.timing.duration_ms > 0

    # # Verify usage fields
    # assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    # assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # # Verify calls
    # calls = log.calls
    # assert len(calls) == 1

    # call = calls[0]

    # assert call.provider == "openai"
    # assert call.client_name == "GPT4oMini"
    # assert call.selected

    # # Verify request/response
    # request = call.http_request
    # assert request is not None
    # print(f"### request.body: {request.body} \n {type(request.body)}", file=sys.stderr)
    # assert isinstance(request.body, dict)
    # assert "messages" in request.body

    # # Verify http response
    # response = call.http_response
    # assert response is None

    # # Verify call timing
    # call_timing = call.timing
    # assert call_timing.start_time_utc_ms > 0
    # assert call_timing.duration_ms is not None and call_timing.duration_ms > 0

    # # Verify call usage
    # call_usage = call.usage
    # assert call_usage.input_tokens is not None and call_usage.input_tokens > 0
    # assert call_usage.output_tokens is not None and call_usage.output_tokens > 0

    # # Verify raw response exists
    # assert log.raw_llm_response is not None

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert Collector.__function_span_count() > 0


# @pytest.mark.asyncio
# async def test_collector_events_present_no_stream_sync():
#     pass


# @pytest.mark.asyncio
# async def test_collector_events_present_stream():
#     gc.collect()

#     print("### function_span_count", Collector.__function_span_count())
#     # garbage collected!
#     # assert Collector.__function_span_count() == 0


# @pytest.fixture(autouse=True)
# def verify_spans():
#     yield

#     gc.collect()
#     print("### function_span_count", Collector.__print_storage())
#     # Verify no spans are leaked after each test
#     # assert Collector.__function_span_count() == 0
