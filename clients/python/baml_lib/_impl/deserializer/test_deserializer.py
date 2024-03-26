from pydantic import BaseModel
from typing import List, Optional
from baml_lib._impl.deserializer import Deserializer, register_deserializer
from enum import Enum
import pytest
import json


@register_deserializer({})
class BasicClass(BaseModel):
    a: int
    b: str


def test_string_from_string() -> None:
    deserializer = Deserializer[str](str)
    res = deserializer.from_string("hello")
    assert "hello" == res


def test_string_from_str_w_quotes() -> None:
    deserializer = Deserializer[str](str)
    res = deserializer.from_string('"hello"')
    assert '"hello"' == res


def test_string_from_object() -> None:
    deserializer = Deserializer[str](str)
    mydict = {"hello": "world"}
    res = deserializer.from_string(json.dumps(mydict))
    # We detect the object and give it pretty printed
    assert json.dumps(mydict, indent=2) == res


def test_string_from_obj_and_string() -> None:
    deserializer = Deserializer[str](str)
    test_str = 'The output is: {"hello": "world"}'
    res = deserializer.from_string(test_str)
    assert test_str == res


def test_string_from_list() -> None:
    deserializer = Deserializer[str](str)
    test_list = ["hello", "world"]
    res = deserializer.from_string(json.dumps(test_list))
    assert json.dumps(test_list) == res


def test_string_from_int() -> None:
    deserializer = Deserializer[str](str)
    res = deserializer.from_string("1")
    assert res == "1"


@register_deserializer({})
class Category(str, Enum):
    ONE = "ONE"
    TWO = "TWO"


def test_enum() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string("TWO")
    assert res == Category.TWO


def test_enum_case_insensitive() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string("two")
    assert res == Category.TWO
    res = deserializer.from_string("TWO")
    assert res == Category.TWO


def test_enum_word_boundaries() -> None:
    deserializer = Deserializer[Category](Category)
    with pytest.raises(Exception):
        deserializer.from_string("artwork")


def test_enum_with_quotes() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string('"TWO"')
    assert res == Category.TWO


def test_enum_missing() -> None:
    deserializer = Deserializer[Category](Category)
    with pytest.raises(Exception):
        deserializer.from_string("THREE")


def test_enum_with_text_before() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string("The output is: TWO")
    assert res == Category.TWO


def test_enum_from_enum_list_single() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string('["TWO"]')
    assert res == Category.TWO


def test_enum_from_enum_list_multi() -> None:
    deserializer = Deserializer[Category](Category)
    res = deserializer.from_string('["TWO", "THREE"]')
    assert res == Category.TWO


def test_enum_list_from_list() -> None:
    deserializer = Deserializer[List[Category]](List[Category])
    res = deserializer.from_string('["TWO"]')
    assert res == [Category.TWO]


# Test case for when LLM writes something like: "k1: The description of k1"
def test_enum_from_string_with_extra_text_after() -> None:
    deserializer = Deserializer[Category](Category)

    res = deserializer.from_string('"ONE: The description of k1"')
    assert res == Category.ONE

    res = deserializer.from_string('"ONE - The description of an enum value"')
    assert res == Category.ONE

    with pytest.raises(Exception):
        deserializer.from_string('"ONE - is the answer, not TWO"')

    with pytest.raises(Exception):
        deserializer.from_string('"ONE. is the answer, not TWO"')

    with pytest.raises(Exception):
        deserializer.from_string('"ONE: is the answer, not TWO"')


@register_deserializer(
    aliases={
        "k1": "ONE",
        "k1: The description of enum value une": "ONE",
        "k-2-3.1_1": "TWO",
        "k-2-3.1_1: The description of enum value deux": "TWO",
        "NUMBER_THREE": "THREE",
    }
)
class CategoryWithAlias(str, Enum):
    ONE = "ONE"
    TWO = "TWO"
    THREE = "THREE"


def test_enum_aliases() -> None:
    deserializer = Deserializer[CategoryWithAlias](CategoryWithAlias)

    res = deserializer.from_string("k1")
    assert res == CategoryWithAlias.ONE

    res = deserializer.from_string("k-2-3.1_1")
    assert res == CategoryWithAlias.TWO

    res = deserializer.from_string("NUMBER_THREE")
    assert res == CategoryWithAlias.THREE


def test_enum_aliases_with_punctuation() -> None:
    deserializer = Deserializer[CategoryWithAlias](CategoryWithAlias)

    res = deserializer.from_string("k1")
    assert res == CategoryWithAlias.ONE

    res = deserializer.from_string("k 2 3 1_1")
    assert res == CategoryWithAlias.TWO

    res = deserializer.from_string("number three")
    assert res == CategoryWithAlias.THREE


def test_enum_aliases_with_extra_text() -> None:
    deserializer = Deserializer[CategoryWithAlias](CategoryWithAlias)
    res = deserializer.from_string("k1: The description of enum value une")
    assert res == CategoryWithAlias.ONE

    # separated by colon
    res = deserializer.from_string("k-2-3.1_1: The description of enum value deux")
    assert res == CategoryWithAlias.TWO

    # separated by whitespace
    res = deserializer.from_string("k-2-3.1_1 The description of enum value deux")
    assert res == CategoryWithAlias.TWO

    # trailing period
    res = deserializer.from_string("k-2-3.1_1. The description of enum value deux")
    assert res == CategoryWithAlias.TWO


def test_enum_aliases_from_multiple_aliases() -> None:
    deserializer = Deserializer[CategoryWithAlias](CategoryWithAlias)

    res = deserializer.from_string("k1: The description of k1, not k-2-3.1_1")
    assert res == CategoryWithAlias.ONE

    # separated by colon
    res = deserializer.from_string("k-2-3.1_1: The description of k-2-3.1_1, not k1")
    assert res == CategoryWithAlias.TWO

    # separated by whitespace
    res = deserializer.from_string("k-2-3.1_1 is the description of k-2-3.1_1, not k1")
    assert res == CategoryWithAlias.TWO

    # trailing period
    res = deserializer.from_string("k-2-3.1_1. is the description of k-2-3.1_1, not k1")
    assert res == CategoryWithAlias.TWO


def test_enum_aliases_from_punctuation() -> None:
    deserializer = Deserializer[CategoryWithAlias](CategoryWithAlias)

    res = deserializer.from_string("k-2-3.1_1")
    assert res == CategoryWithAlias.TWO

    res = deserializer.from_string("k.2.3.1.1")
    assert res == CategoryWithAlias.TWO


def test_enum_list_from_string_with_aliases() -> None:
    deserializer = Deserializer[List[CategoryWithAlias]](List[CategoryWithAlias])
    res = deserializer.from_string('["k1", "k-2-3.1_1", "NUMBER_THREE"]')
    assert res == [
        CategoryWithAlias.ONE,
        CategoryWithAlias.TWO,
        CategoryWithAlias.THREE,
    ]


@register_deserializer({})
class BasicObj(BaseModel):
    foo: str


def test_obj_from_str() -> None:
    deserializer = Deserializer[BasicObj](BasicObj)
    test_obj = {"foo": "bar"}
    res = deserializer.from_string(json.dumps(test_obj))
    assert "bar" == res.foo


def test_obj_from_str_with_other_text() -> None:
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('The output is: {"foo": "bar"}')
    assert "bar" == res.foo


def test_obj_from_str_with_quotes() -> None:
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('{"foo": "[\\"bar\\"]"}')
    assert '["bar"]' == res.foo


def test_obj_from_str_with_nested_json_string() -> None:
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string('{"foo": "{\\"foo\\": [\\"bar\\"]}"}')
    assert '{\n  "foo": [\n    "bar"\n  ]\n}' == res.foo


def test_obj_from_str_with_nested_complex_string2() -> None:
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
    assert res == test_value


def test_obj_from_str_with_string_foo() -> None:
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
    assert res.foo == test_value


def test_json_thing() -> None:
    llm_value = '{\n    "foo": "This is a sample string with **markdown** that includes a JSON blob: `{\\"name\\": \\"John\\", \\"age\\": 30}`. Please note that the JSON blob inside the string is escaped to fit into the string type."\n}'
    expected = json.loads(llm_value)
    deserializer = Deserializer[BasicObj](BasicObj)
    res = deserializer.from_string(llm_value)
    print("res", res)
    assert res.foo == expected["foo"]


@register_deserializer({})
class ObjOptionals(BaseModel):
    foo: Optional[str] = None


def test_object_with_empty_input() -> None:
    object = {
        "foo": "",
    }
    deserializer = Deserializer[ObjOptionals](ObjOptionals)
    res = deserializer.from_string(json.dumps(object))
    assert res.foo == ""
    obj2 = {
        "foo": None,
    }
    res = deserializer.from_string(json.dumps(obj2))
    assert res.foo is None


@register_deserializer({})
class BasicClass2(BaseModel):
    one: str
    two: str


def test_object_from_str_with_quotes() -> None:
    deserializer = Deserializer[BasicClass2](BasicClass2)
    test_obj = {
        "one": "hello 'world'",
        "two": 'double hello "world"',
    }
    res = deserializer.from_string(json.dumps(test_obj))
    assert test_obj["one"] == res.one


@register_deserializer({})
class BasicClass3(BaseModel):
    one: str
    two: Optional[str] = None


def test_object_from_str_missing_optional_key() -> None:
    deserializer = Deserializer[BasicClass3](BasicClass3)
    test_obj = {
        "one": "hello",
    }
    res = deserializer.from_string(json.dumps(test_obj))
    assert test_obj["one"] == res.one
    assert res.two is None


def test_obj_from_json_markdown() -> None:
    test_value = """Here is how you can build the API call:
```json
{
    "one": "hi",
    "two": "hello"
}
```

```json
    {
        "test2": {
            "key2": "value"
        },
        "test21": [
        ]    
    }
```
"""
    deserializer = Deserializer[BasicClass2](BasicClass2)
    res = deserializer.from_string(test_value)
    assert res.one == "hi"


@register_deserializer({})
class BasicWithList(BaseModel):
    a: int
    b: str
    c: List[str]


def test_complex_obj_from_string() -> None:
    deserializer = Deserializer[BasicWithList](BasicWithList)
    test_obj = {
        "a": 1,
        "b": "hello",
        "c": ["world"],
    }
    res = deserializer.from_string(json.dumps(test_obj))
    assert res.a == 1
    assert res.b == "hello"
    assert res.c == ["world"]


@register_deserializer({})
class Child(BaseModel):
    hi: str


@register_deserializer({})
class Parent(BaseModel):
    child: Child


def test_complex_obj_from_string2() -> None:
    deserializer = Deserializer[Parent](Parent)
    test_obj = {"child": {"hi": "hello"}}
    res = deserializer.from_string(json.dumps(test_obj))
    assert res.child.hi == "hello"


def test_complex_obj_from_string_json_markdown() -> None:
    deserializer = Deserializer[Parent](Parent)
    test_str = """Here is how you can build the API call:
```json
{
    "child": {
        "hi": "hello"
    }
}
```
and this
```json
{
    "child": {
        "hi": "twooo"
    }
}
"""
    res = deserializer.from_string(test_str)
    assert res.child.hi == "hello"


def test_complex_obj_from_json_with_prefix() -> None:
    deserializer = Deserializer[Parent](Parent)
    test_str = """Here is how you can build the API call:

{
    "child": {
        "hi": "hello"
    }
}
"""
    res = deserializer.from_string(test_str)
    assert res.child.hi == "hello"


def test_list_from_string() -> None:
    deserializer = Deserializer[List[str]](List[str])
    test_obj = ["hello", "world"]
    res = deserializer.from_string(json.dumps(test_obj))
    assert res == ["hello", "world"]


def test_list_object_from_string() -> None:
    deserializer = Deserializer[List[BasicClass]](List[BasicClass])
    test_obj = [{"a": 1, "b": "hello"}, {"a": 2, "b": "world"}]
    res = deserializer.from_string(json.dumps(test_obj))
    assert res == [BasicClass(a=1, b="hello"), BasicClass(a=2, b="world")]


@register_deserializer({})
class BasicClass4(BaseModel):
    a: int
    b: List[str]


def test_list_object_from_string_with_other_text() -> None:
    deserializer = Deserializer[List[BasicClass4]](List[BasicClass4])
    # This should pass even though b is not a list in the string.
    test_obj = [{"a": 1, "b": "hello"}, {"a": 2}]
    res = deserializer.from_string(f"The output is: {json.dumps(test_obj)}")
    assert res == [BasicClass4(a=1, b=["hello"]), BasicClass4(a=2, b=[])]
