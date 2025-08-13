"""
Baml VM / compiler / expression functions tests.
"""

# import pytest
# 
# from ..baml_client import b
# from ..baml_client.sync_client import b as sync_b
# from ..baml_client.runtime import disassemble
# 
# 
# def test_return_one():
#     assert sync_b.ReturnOne() == 1
# 
# 
# def test_return_number():
#     assert sync_b.ReturnNumber(42) == 42
# 
# 
# def test_call_return_one():
#     assert sync_b.CallReturnOne() == 1
# 
# 
# def test_chained_calls():
#     assert sync_b.ChainedCalls() == 1
# 
# 
# def test_store_fn_call_in_local_var():
#     assert sync_b.StoreFnCallInLocalVar(42) == 42
# 
# 
# def test_bool_to_int_with_if_else():
#     assert sync_b.BoolToIntWithIfElse(True) == 1
#     assert sync_b.BoolToIntWithIfElse(False) == 0
# 
# 
# def test_return_else_if_expr():
#     disassemble(sync_b.ReturnElseIfExpr)
#     assert sync_b.ReturnElseIfExpr(True, False) == 1
#     assert sync_b.ReturnElseIfExpr(False, True) == 2
#     assert sync_b.ReturnElseIfExpr(False, False) == 3
# 
# 
# def test_assign_else_if_expr():
#     disassemble(sync_b.AssignElseIfExpr)
#     assert sync_b.AssignElseIfExpr(True, False) == 1
#     assert sync_b.AssignElseIfExpr(False, True) == 2
#     assert sync_b.AssignElseIfExpr(False, False) == 3
# 
# 
# def test_normal_else_if_stmt():
#     disassemble(sync_b.NormalElseIfStmt)
#     assert sync_b.NormalElseIfStmt(True, False) == 0
# 
# 
# @pytest.mark.asyncio
# async def test_llm_call_in_expr_fn():
#     assert await b.ReturnNumberCallingLlm(42) == 42
# 
# 
# @pytest.mark.asyncio
# async def test_store_llm_call_in_local_var():
#     assert await b.StoreLlmCallInLocalVar(42) == 42
# 
# 
# @pytest.mark.asyncio
# async def test_bool_to_int_with_if_else_calling_llm():
#     disassemble(b.BoolToIntWithIfElseCallingLlm)
#     assert await b.BoolToIntWithIfElseCallingLlm(True) == 1
#     assert await b.BoolToIntWithIfElseCallingLlm(False) == 0
# 