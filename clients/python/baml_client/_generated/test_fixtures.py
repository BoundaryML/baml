"""
A collection of fixtures for testing BAML functions.

These automatically parameterize a test function over every defined variant.

Example usage:

@baml.MyFunction.test
async def test_logic(MyFunctionTestHandler: IMyFunction) -> None:
    result = await MyFunctionTestHandler(foo=42, bar="baz")
    ...

Note the parameter name must match the name of the function being tested.

See pytest documentation for more information on fixtures:
https://docs.pytest.org/en/latest/fixture.html
"""

from _pytest.fixtures import FixtureRequest
from .baml_types import IMyFunction
from .baml_client import baml


def MyFunctionImpl(request: FixtureRequest) -> IMyFunction:
    """
    To use this fixture, add this to your test.
    Note the parameter name must match the name of this fixture.


    ```python
    @baml.MyFunction.test
    async def test_logic(MyFunctionImpl: IMyFunction) -> None:
        result = await MyFunctionImpl(foo=42, bar="baz")
        ...
    ```

    See the docstring for baml.MyFunction.test for more information.


    See pytest documentation for more information on fixtures:
    https://docs.pytest.org/en/latest/fixture.html
    """
    return baml.MyFunction.get_impl(request.param).run
