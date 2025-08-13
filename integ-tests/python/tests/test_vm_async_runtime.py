"""
Baml VM / compiler / expression functions with LLM calls. Ignored in CI.
"""

import pytest

from ..baml_client import b
from ..baml_client.runtime import disassemble


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
