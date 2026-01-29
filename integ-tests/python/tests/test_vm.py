"""
Baml VM / compiler / expression functions tests.

Important: No LLM calls here, this will run in CI.
"""

from ..baml_client.sync_client import b
from ..baml_client.runtime import disassemble
from ..baml_client.types import Category


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


def test_iterative_fibonacci():
    assert b.IterativeFibonacci(0) == 1
    assert b.IterativeFibonacci(1) == 1
    assert b.IterativeFibonacci(2) == 1
    assert b.IterativeFibonacci(3) == 2
    assert b.IterativeFibonacci(4) == 3
    assert b.IterativeFibonacci(5) == 5
    assert b.IterativeFibonacci(6) == 8
    assert b.IterativeFibonacci(7) == 13
    assert b.IterativeFibonacci(8) == 21


def test_sum_array():
    assert b.SumArray([1, 2, 3]) == 6
    assert b.SumArray([1, 2, 3, 4, 5]) == 15
    assert b.SumArray([]) == 0


def test_sum_from_to():
    assert b.SumFromTo(1, 10) == 55


def test_return_category():
    assert b.ReturnCategory(Category.Refund) == Category.Refund
    assert b.ReturnCategory(Category.CancelOrder) == Category.CancelOrder
    assert b.ReturnCategory(Category.TechnicalSupport) == Category.TechnicalSupport
    assert b.ReturnCategory(Category.AccountIssue) == Category.AccountIssue
    assert b.ReturnCategory(Category.Question) == Category.Question


# def test_return_image_from_url():
#     url = "https://i.imgur.com/93fWs5R.png"

#     # Image created within BAML.
#     img = b.ReturnImageFromUrl(url)

#     assert img.is_url()
#     assert img.as_url() == url

# def test_home_env_var_length():
#     assert not b.HomeEnvVarIsEmpty()
