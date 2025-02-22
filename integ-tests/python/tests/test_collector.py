import uuid
import json
import os
import time
from typing import List, Optional
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64
from openai import ChatCompletion

load_dotenv()
import baml_py
from baml_py import errors

from ..baml_client import b
from ..baml_client.tracing import trace
from baml_py import Collector, FunctionLog
import gc
import sys


# async def collector_test1():

#     # Test that the collector can be initialized
#     collector = Collector(id="my-collector")

#     # TODO:
#     try:
#         try:
#             res = await b.TestOpenAIGPT4oMini(
#                 "hi there", baml_options={"collector": collector}
#             )
#         except Exception as e:
#             print("###### error ######", file=sys.stderr)
#             print(e, file=sys.stderr)
#         res2 = await b.TestOpenAIGPT4oMini(
#             "hi there", baml_options={"collector": collector}
#         )

#     except Exception as e:
#         print("###### error ######", file=sys.stderr)
#         print(e, file=sys.stderr)
#     finally:
#         print("### function_span_count", Collector.__function_span_count())
#         assert Collector.__function_span_count() == 2  # two functions were called

#         print("###### collector ######", file=sys.stderr)
#         events = collector.events()
#         print("### event size", len(events), file=sys.stderr)
#         # print(events[0].calls()[0])
#         # print(events[0].calls()[0].response())
#         print("###### res ######")

#         print("##### kicking off gc ######", file=sys.stderr)
#         gc.collect()

#     print("### events2", file=sys.stderr)
#     events2 = collector.events()
#     print("### event size2", len(events2), file=sys.stderr)


# @pytest.mark.asyncio
# async def test_collector():
#     await collector_test1()
#     print("### function_span_count", Collector.__function_span_count())
#     # garbage collected!
#     assert Collector.__function_span_count() == 0


@pytest.mark.asyncio
async def test_collector_events_present():
    print("### function_span_count", Collector.__function_span_count())
    # garbage collected!
    assert Collector.__function_span_count() == 0

    collector = Collector(id="my-collector")
    function_logs = collector.logs
    print("### function_logs", function_logs, file=sys.stderr)
    assert len(function_logs) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})

    function_logs = collector.logs
    print("### function_logs2", function_logs, file=sys.stderr)
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
    assert call.client_name == "gpt-4"
    assert call.selected

    # Verify request/response
    request = call.http_request
    assert request is not None
    assert "messages" in request.body
    assert "content" in request.body

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

    # Verify metadata
    assert isinstance(log.metadata, dict)
