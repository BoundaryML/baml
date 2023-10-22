from typing import Type
from ._generated import baml
from ._generated.baml_types import MyFunctionInferface


@baml.MyFunction.register_impl("foo")
def foo(*, foo: int, bar: str) -> str:
    # This is the implementation for foo.
    return f"foo: {foo}, bar: {bar}"


@baml.MyFunction.test(exclude_impl=["foo"])
def test_logic(MyFunctionTestHandler: MyFunctionInferface) -> None:
    result = MyFunctionTestHandler(foo=42, bar="baz")
    assert (
        result == "foo: 42, bar: baz"
    ), f"Expected 'foo: 42, bar: baz' but got {result}"


@baml.MyFunction.test
def test_logic_2(MyFunctionTestHandler: MyFunctionInferface) -> None:
    result = MyFunctionTestHandler(foo=42, bar="baz")
    assert (
        result == "foo: 42, bar: baz"
    ), f"Expected 'foo: 42, bar: baz' but got {result}"


@baml.MyFunction.test
class TestMethod:
    def test_a(self, MyFunctionHandler: MyFunctionInferface) -> None:
        pass
