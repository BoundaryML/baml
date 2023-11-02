import json
import typing
from .loader import from_string
from .wrappers import (
    DictRawWrapper,
    ListRawWrapper,
    RawBaseWrapper,
    RawStringWrapper,
)


import pytest
from _pytest.fixtures import FixtureRequest

Serializer = typing.Callable[[typing.Any], str]


@pytest.fixture(params=[str, json.dumps, lambda x: f'"{json.dumps(x)}"'])
def serializer(request: FixtureRequest) -> Serializer:
    return typing.cast(Serializer, request.param)


@pytest.mark.parametrize(
    "test_case",
    [{"key": "value"}, {"key1": "value1", "key2": "value2"}],
)
def test_from_string_dict(
    test_case: typing.Mapping[typing.Any, typing.Any], serializer: Serializer
) -> None:
    parsed = from_string(serializer(test_case))
    assert isinstance(parsed, DictRawWrapper), parsed


@pytest.mark.parametrize("test_case", [[1, 2, 3], [1, 2, 3, 4, 5]])
def test_from_string_list(
    test_case: typing.List[typing.Any], serializer: Serializer
) -> None:
    assert isinstance(from_string(serializer(test_case)), ListRawWrapper)


@pytest.mark.parametrize("test_case", ["string", "another string"])
def test_from_string_raw_string(test_case: str) -> None:
    assert isinstance(from_string(test_case), RawStringWrapper)


@pytest.mark.parametrize("test_case", [123, True, False, 1.0, 0.0])
def test_from_string_raw_base(test_case: typing.Any, serializer: Serializer) -> None:
    assert isinstance(from_string(serializer(test_case)), RawBaseWrapper)
