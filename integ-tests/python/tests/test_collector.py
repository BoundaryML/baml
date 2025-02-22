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


async def collector_test1():

    # Test that the collector can be initialized
    collector = Collector(id="my-collector")

    # TODO:
    try:
        try:
            res = await b.TestOpenAIGPT4oMini(
                "hi there", baml_options={"collector": collector}
            )
        except Exception as e:
            print("###### error ######", file=sys.stderr)
            print(e, file=sys.stderr)
        res2 = await b.TestOpenAIGPT4oMini(
            "hi there", baml_options={"collector": collector}
        )

    except Exception as e:
        print("###### error ######", file=sys.stderr)
        print(e, file=sys.stderr)
    finally:
        print("### function_span_count", Collector.__function_span_count())
        assert Collector.__function_span_count() == 2  # two functions were called

        print("###### collector ######", file=sys.stderr)
        events = collector.events()
        print("### event size", len(events), file=sys.stderr)
        # print(events[0].calls()[0])
        # print(events[0].calls()[0].response())
        print("###### res ######")

        print("##### kicking off gc ######", file=sys.stderr)
        gc.collect()

    print("### events2", file=sys.stderr)
    events2 = collector.events()
    print("### event size2", len(events2), file=sys.stderr)


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
    events = collector.events()
    assert len(events) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})

    events = collector.events()
    assert len(events) == 1
    
    # one call was made
    events
