"""
Baml VM / compiler / expression functions tests.
"""

import pytest

from ..baml_client import b
from ..baml_client.sync_client import b as sync_b


def test_basic_expr_fn_call():
    assert sync_b.ReturnOne() == 1


def test_expr_fn_with_arg():
    assert sync_b.ReturnNumber(42) == 42


def test_expr_fn_nested_call():
    assert sync_b.CallReturnOne() == 1


def test_expr_fn_chained_calls():
    assert sync_b.ChainedCalls() == 1


@pytest.mark.asyncio
async def test_basic_llm_call_in_expr_fn():
    assert await b.CallLlmReturnNumber(42) == 42
