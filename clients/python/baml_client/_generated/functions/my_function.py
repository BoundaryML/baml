import typing


from ..._impl.functions import BaseBAMLFunction

ImplName = typing.Literal["foo", "bar"]


@typing.runtime_checkable
class MyFunctionInferface(typing.Protocol):
    """
    This is the interface for a function.

    Args:
        foo (int): Description for the foo parameter.
        bar (str): Description for the bar parameter.

    Returns:
        str: Description for the return type.
    """

    def __call__(self, *, foo: int, bar: str) -> typing.Awaitable[str]:
        ...


class BAMLMyFunction(BaseBAMLFunction[str]):
    def __init__(self) -> None:
        super().__init__("MyFunction", MyFunctionInferface)
