import json
import pytest
from pydantic import BaseModel
import typing
from typing import Any

from ..stringify_class import FieldDescription
from .. import (
    StringifyFloat,
    StringifyCtx,
    StringifyClass,
    StringifyInt,
    StringifyString,
    StringifyList,
    StringifyOptional,
    StringifyBool,
    StringifyChar,
)


class TestPrimitive:
    @pytest.mark.parametrize(
        "actual,expected",
        [
            (1, "1"),
            (1.0, "1.0"),
            (True, "True"),
            (False, "False"),
            (None, "None"),
            ("hello", "hello"),
            ("'hello'", "hello"),
            ('"hello"', "hello"),
            ("'''hello'''", "hello"),
            ('"""hello"""', "hello"),
            ("   hello", "hello"),
            # Array to string
            (["hello"], "hello"),
            (["hello", "world"], "hello"),
            ('["hello"]', "hello"),
            ('["hello", "world"]', "hello"),
            # Tuple to string
            (("hello",), "hello"),
            (("hello", "world"), "hello"),
            # Tuples as strings aren't supported
            ('("hello")', '("hello")'),
            ('("hello", "world")', '("hello", "world")'),
            # Object to string
            ({"hello": "world"}, "world"),
            ('{"hello": "world"}', "world"),
            ('{"hello": "world", "hello2": "world2"}', "world"),
            ({"hello": "world", "hello2": "world2"}, "world"),
        ],
    )
    def test_string(self, actual: Any, expected: str) -> None:
        with StringifyCtx():
            x = StringifyString().parse(actual)
            assert isinstance(x, str)
            if '"' in expected:
                alt_expected = expected.replace('"', "'")
            elif "'" in expected:
                alt_expected = expected.replace("'", '"')
            else:
                alt_expected = None
            if alt_expected:
                assert x == expected or x == alt_expected
            else:
                assert x == expected

    @pytest.mark.parametrize(
        "actual,expected",
        [
            (1, "1"),
            (1.0, "1"),
            (True, "T"),
            (False, "F"),
            (None, "N"),
            ("hello", "h"),
            ("'hello'", "h"),
            ('"hello"', "h"),
            ("'''hello'''", "h"),
            ('"""hello"""', "h"),
            ("   hello", "h"),
            # Array to string
            (["hello"], "h"),
            (["hello", "world"], "h"),
            ('["hello"]', "h"),
            ('["hello", "world"]', "h"),
            # Tuple to string
            (("hello",), "h"),
            (("hello", "world"), "h"),
            # Tuples as strings aren't supported
            ('("hello")', "("),
            ('("hello", "world")', "("),
            # Object to string
            ({"hello": "world"}, "w"),
            ('{"hello": "world"}', "w"),
            ('{"hello": "world", "hello2": "1234"}', "w"),
            ({"hello": "world", "hello2": "1234"}, "w"),
        ],
    )
    def test_char(self, actual: Any, expected: str) -> None:
        with StringifyCtx():
            x = StringifyChar().parse(actual)
            assert isinstance(x, str)
            if '"' in expected:
                alt_expected = expected.replace('"', "'")
            elif "'" in expected:
                alt_expected = expected.replace("'", '"')
            else:
                alt_expected = None
            if alt_expected:
                assert x == expected or x == alt_expected
            else:
                assert x == expected

    @pytest.mark.parametrize(
        "actual,expected",
        [
            (1, 1),
            (1.1, 1),
            (1.6, 1),
            (True, 1),
            (False, 0),
            ([1, 2, 3], 1),
        ],
    )
    @pytest.mark.parametrize("l", [None, json.dumps, str])
    def test_int(self, l: Any, actual: Any, expected: int) -> None:  # noqa: E741
        if l:
            actual = l(actual)
        with StringifyCtx():
            x = StringifyInt().parse(actual)
            assert isinstance(x, int)
            assert x == expected

    @pytest.mark.parametrize(
        "actual,expected",
        [
            (1, 1),
            (1.1, 1.1),
            (1.6, 1.6),
            (True, 1),
            (False, 0),
            ([1.1, 2.11, 3], 1.1),
        ],
    )
    @pytest.mark.parametrize("l", [None, json.dumps, str])
    def test_float(self, l: Any, actual: Any, expected: int) -> None:  # noqa: E741
        if l:
            actual = l(actual)
        with StringifyCtx():
            x = StringifyFloat().parse(actual)
            assert isinstance(x, float)
            assert x == expected

    @pytest.mark.parametrize(
        "actual,expected",
        [
            (1, True),
            (1.1, True),
            (True, True),
            (False, False),
            ([1.1, 2.11, 3], True),
            ([], False),
            ([1], True),
            ([1, 2], True),
            ([0], False),
            ([True], True),
            ([False], False),
            ("true", True),
            ("false", False),
        ],
        ids=[
            "int",
            "float",
            "True",
            "False",
            "list",
            "empty list",
            "list with 1 element",
            "list with 2 elements",
            "list with 0",
            "list with True",
            "list with False",
            "string true",
            "string false",
        ],
    )
    @pytest.mark.parametrize("l", [None, json.dumps, str])
    def test_bool(self, l: Any, actual: Any, expected: int) -> None:  # noqa: E741
        if l:
            actual = l(actual)
        with StringifyCtx():
            x = StringifyBool().parse(actual)
            assert isinstance(x, bool)
            assert x == expected


class TestClass:
    def test_class_a(self) -> None:
        class ModelA(BaseModel):
            a: int
            b: str
            c: typing.List[int]
            d: typing.Optional[str]

        with StringifyCtx():
            clx = StringifyClass(
                model=ModelA,
                values={
                    "a": FieldDescription(
                        name="a", description=None, type_desc=StringifyInt()
                    ),
                    "b": FieldDescription(
                        name="b", description=None, type_desc=StringifyString()
                    ),
                    "c": FieldDescription(
                        name="c",
                        description=None,
                        type_desc=StringifyList(StringifyInt()),
                    ),
                    "d": FieldDescription(
                        name="d",
                        description=None,
                        type_desc=StringifyOptional(StringifyString()),
                    ),
                },
                updates={},
            )

        x = clx.parse("""{"a": 1, "b": '2', "c": [3, 4], "d": '5'}""")
        assert x.a == 1
        assert x.b == "2"
        assert x.c == [3, 4]
        assert x.d == "5"

        del x
