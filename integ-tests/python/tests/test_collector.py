import uuid
import json
import os
import time
from typing import List, Optional
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64

load_dotenv()
import baml_py
from baml_py import errors

from ..baml_client import b
from ..baml_client.tracing import trace
from baml_py import Collector, FunctionLog
import gc
import sys


@pytest.mark.asyncio
async def test_collector():
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

    # TODO: expose a helper to access global log storage


# @trace(require_id=True)
# def function():
#     print("hi")

# def root():
#     id, content = function()
#     # no way to get id
