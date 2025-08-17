"""
Baml VM / compiler / expression functions tests.

Important: No LLM calls here, this will run in CI.
"""

from ..baml_client.sync_client import b
from ..baml_client.runtime import disassemble


def test_return_one():
    assert b.ReturnOne() == 1


def test_return_number():
    assert b.ReturnNumber(42) == 42


def test_call_return_one():
    assert b.CallReturnOne() == 1


def test_chained_calls():
    assert b.ChainedCalls() == 1


def test_store_fn_call_in_local_var():
    assert b.StoreFnCallInLocalVar(42) == 42


def test_bool_to_int_with_if_else():
    assert b.BoolToIntWithIfElse(True) == 1
    assert b.BoolToIntWithIfElse(False) == 0


def test_return_else_if_expr():
    disassemble(b.ReturnElseIfExpr)
    assert b.ReturnElseIfExpr(True, False) == 1
    assert b.ReturnElseIfExpr(False, True) == 2
    assert b.ReturnElseIfExpr(False, False) == 3


def test_assign_else_if_expr():
    disassemble(b.AssignElseIfExpr)
    assert b.AssignElseIfExpr(True, False) == 1
    assert b.AssignElseIfExpr(False, True) == 2
    assert b.AssignElseIfExpr(False, False) == 3


def test_normal_else_if_stmt():
    disassemble(b.NormalElseIfStmt)
    assert b.NormalElseIfStmt(True, False) == 0
