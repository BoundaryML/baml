import json
import typing
from .loader import from_string
from .wrappers import (
    DictRawWrapper,
    ListRawWrapper,
    RawBaseWrapper,
    RawStringWrapper,
)
from .raw_wrapper import RawWrapper
from ..diagnostics import Diagnostics


import pytest
from _pytest.fixtures import FixtureRequest

Serializer = typing.Callable[[typing.Any], str]


@pytest.fixture(params=[str, json.dumps])
def serializer(request: FixtureRequest) -> Serializer:
    return typing.cast(Serializer, request.param)


@pytest.mark.parametrize(
    "test_case",
    [{"key": "value"}, {"key1": "value1", "key2": "value2"}],
)
def test_from_string_dict(
    test_case: typing.Mapping[typing.Any, typing.Any], serializer: Serializer
) -> None:
    print("\n----- test_case", test_case)
    item = serializer(test_case)
    print("item", item)
    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, DictRawWrapper), parsed


@pytest.mark.parametrize("test_case", [[1, 2, 3], [1, 2, 3, 4, 5]])
def test_from_string_list(
    test_case: typing.List[typing.Any], serializer: Serializer
) -> None:
    item = serializer(test_case)
    d = Diagnostics(item)
    assert isinstance(from_string(item, d), ListRawWrapper)


@pytest.mark.parametrize("test_case", ["string", "another string"])
def test_from_string_raw_string(test_case: str) -> None:
    d = Diagnostics(test_case)
    assert isinstance(from_string(test_case, d), RawStringWrapper)


@pytest.mark.parametrize("test_case", [123, True, False, 1.0, 0.0])
def test_from_string_raw_base(test_case: typing.Any, serializer: Serializer) -> None:
    item = serializer(test_case)
    d = Diagnostics(item)
    assert isinstance(from_string(item, d), RawBaseWrapper)


def test_hidden_object() -> None:
    item = """
    ```json
    {
        "test": {
            "key": "value"
        },
        "test2": [
        ]    
    }
    ```
    """

    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, RawStringWrapper)
    keys: typing.Set[str] = set()
    for k, v in parsed.as_dict():
        assert isinstance(k, RawStringWrapper), k
        key = k.as_str(inner=True)
        assert key is not None, k
        keys.add(key)
        if key == "test":
            assert isinstance(v.as_dict(), typing.ItemsView)
        elif key == "test2":
            assert isinstance(v, ListRawWrapper)
    assert len(keys) == 2, keys


def test_object_with_newlines_1() -> None:
    item = """
    {
        "test": {
            "key": "value
As an amazing person
with lines"
        },
        "test2": [
        ]    
    }
    """

    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, DictRawWrapper), parsed

    keys: typing.Set[str] = set()
    for k, v in parsed.as_dict():
        assert isinstance(k, RawStringWrapper), k
        key = k.as_str(inner=True)
        assert key is not None, k
        keys.add(key)
        if key == "test":
            assert isinstance(v, DictRawWrapper)
            assert v.as_self()["key"] == "value\nAs an amazing person\nwith lines"
        elif key == "test2":
            assert isinstance(v, ListRawWrapper)
    assert len(keys) == 2, keys


def test_object_with_newlines_with_quotes() -> None:
    item = """
    {
        "test": {
            "key": "value\\"
As an amazing person
\\" with lines"
        },
        "test2": [
        ]    
    }
    """

    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, DictRawWrapper), parsed
    keys: typing.Set[str] = set()
    for k, v in parsed.as_dict():
        assert isinstance(k, RawStringWrapper), k
        key = k.as_str(inner=True)
        assert key is not None, k
        keys.add(key)
        if key == "test":
            assert isinstance(v, DictRawWrapper)
            assert v.as_self()["key"] == 'value"\nAs an amazing person\n" with lines'
        elif key == "test2":
            assert isinstance(v, ListRawWrapper)
    assert len(keys) == 2, keys


def test_object_with_newlines_with_quotes_and_good_newlines() -> None:
    item = """
    {
        "test": {
            "key": "va\\nlue\\"
As an amazing person
\\" with lines"
        },
        "test2": [
        ]    
    }
    """

    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, DictRawWrapper), parsed
    keys: typing.Set[str] = set()
    for k, v in parsed.as_dict():
        assert isinstance(k, RawStringWrapper), k
        key = k.as_str(inner=True)
        assert key is not None, k
        keys.add(key)
        if key == "test":
            assert isinstance(v, DictRawWrapper)
            assert v.as_self()["key"] == 'va\nlue"\nAs an amazing person\n" with lines'
        elif key == "test2":
            assert isinstance(v, ListRawWrapper)
    assert len(keys) == 2, keys


def test_hidden_list() -> None:
    item = """
    ```json
    [
        ["test", {
            "key": "value"
        }],
        "test2", [
        ]   
    ]
    ```
    """

    d = Diagnostics(item)
    parsed = from_string(item, d)
    assert isinstance(parsed, RawStringWrapper)
    values: typing.List[RawWrapper] = []
    for i, v in enumerate(parsed.as_list()):
        values.append(v)
        if i == 0:
            assert isinstance(v, ListRawWrapper)
        elif i == 1:
            assert isinstance(v, RawStringWrapper)
        elif i == 2:
            assert isinstance(v, ListRawWrapper)

    assert len(values) == 3, values
