from pydantic import BaseModel
from typing import List
from baml_lib._impl.deserializer import Deserializer, register_deserializer
from enum import Enum
import pytest
import json


class BasicClass(BaseModel):
    a: int
    b: str


class BasicWithList(BaseModel):
    a: int
    b: str
    c: List[str]


class BasicWithList2(BaseModel):
    a: int
    b: str
    c: List[BasicClass]


def test_string_from_string():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string("hello")
    assert res == "hello"


def test_string_from_str_w_quotes():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string('"hello"')
    assert res == '"hello"'


def test_string_from_object():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string('{"hello": "world"}')
    assert res == '{"hello": "world"}'


def test_string_from_obj_and_string():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string('The output is: {"hello": "world"}')
    assert res == 'The output is: {"hello": "world"}'


def test_string_from_list():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string('["hello", "world"]')
    assert res == '["hello", "world"]'


def test_string_from_int():
    deserializer = Deserializer[str](str)
    res = deserializer.from_string("1")
    assert res == "1"


@register_deserializer({})
class Category(str, Enum):
    ONE = "ONE"
    TWO = "TWO"


def test_enum():
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string("TWO")
    assert res == Category.TWO


def test_enum_missing():
    deserializer = Deserializer[Category](Category)
    with pytest.raises(Exception):
        deserializer.from_string("THREE")


@register_deserializer({})
class BasicObj(BaseModel):
    foo: str


def test_obj_from_str():
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('{"foo": "bar"}')
    assert res.foo == "bar"


def test_obj_from_str_with_quotes():
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('{"foo": "[\\"bar\\"]"}')
    assert res.foo == '["bar"]'


def test_obj_from_str_with_nested_foo():
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('{"foo": "{\\"foo\\": [\\"bar\\"]}"}')
    assert res.foo == '{"foo": "["bar"]}'


def test_obj_from_str_with_nested_foo2():
    test_value = """Here is how you can build the API call:
```json
{
    "foo": {
        "foo": [
            "bar"
        ]
    }
}
```
"""
    deserializer = Deserializer[str](str)
    res = deserializer.from_string(test_value)
    assert res == test_value.strip()


def test_obj_from_str_with_string_foo():
    test_value = """Here is how you can build the API call:
```json
{
    "hello": {
        "world": [
            "bar"
        ]
    }
}
```
"""
    # Note LLM should add these (\\) too for the value of foo.
    test_value_str = test_value.replace("\n", "\\n").replace('"', '\\"')
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string(f'{{"foo": "{test_value_str}"}}')
    print("res", res)
    assert res.foo == test_value.strip()


def test_json_thing():
    llm_value = '{\n    "foo": "This is a sample string with **markdown** that includes a JSON blob: `{\\"name\\": \\"John\\", \\"age\\": 30}`. Please note that the JSON blob inside the string is escaped to fit into the string type."\n}'
    expected = json.loads(llm_value)
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string(llm_value)
    print("res", res)
    assert res.foo == expected["foo"]
