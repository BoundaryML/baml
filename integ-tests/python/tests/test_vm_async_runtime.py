"""
Baml VM / compiler / expression functions with LLM calls. Ignored in CI.
"""

import pytest
from baml_py import Image

from ..baml_client import b
from ..baml_client.runtime import disassemble
from ..baml_client.types import DummyJsonTodo


@pytest.mark.asyncio
async def test_llm_call_in_expr_fn():
    assert await b.ReturnNumberCallingLlm(42) == 42


@pytest.mark.asyncio
async def test_store_llm_call_in_local_var():
    assert await b.StoreLlmCallInLocalVar(42) == 42


@pytest.mark.asyncio
async def test_bool_to_int_with_if_else_calling_llm():
    disassemble(b.BoolToIntWithIfElseCallingLlm)
    assert await b.BoolToIntWithIfElseCallingLlm(True) == 1
    assert await b.BoolToIntWithIfElseCallingLlm(False) == 0


@pytest.mark.asyncio
async def test_call_llm_describe_image():
    # Call an expression function that calls an LLM function to check if the
    # media type is passed correctly.
    description = await b.CallLlmDescribeImage(
        Image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png")
    )

    assert "ogre" in description.lower()


@pytest.mark.asyncio
async def test_baml_fetch_as():
    disassemble(b.ExecFetchAs)

    result = await b.ExecFetchAs("https://dummyjson.com/todos/1")

    assert result == DummyJsonTodo(
        id=1,
        todo="Do something nice for someone you care about",
        completed=False,
        userId=152,
    )
