import time
from typing import List
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64

load_dotenv()
import baml_py
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b

from ..baml_client import partial_types

from ..baml_client.tracing import trace, set_tags, flush, on_log_event
from ..baml_client.type_builder import TypeBuilder
import datetime
import concurrent.futures
import asyncio
import random


@pytest.mark.asyncio
async def test_accepts_subclass_of_baml_type():
    print("calling with class")
    _ = await b.ExtractResume("hello")
