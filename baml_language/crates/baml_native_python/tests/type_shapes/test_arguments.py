"""How arguments are read: the same answers from today's SDK and from the
native bindings. These run against both (PYTHONPATH=.../vm or .../native),
so each answer is today's, checked, not assumed."""

import pytest

import baml_sdk  # noqa: F401
from baml_bridge.errors import BamlError
from baml_sdk.bridge_test import (
    Mood,
    Point,
    Polarity,
    float_or_literal,
    round_trip_list_union,
    mood_or_text,
    note_or_tag,
    point_or_map,
    round_trip_point,
    text_or_mood,
)
from baml_sdk.enums import Sentiment, round_trip_sentiment
from baml_sdk.primitives import round_trip_float


async def test_an_enum_member_is_the_enum_even_where_string_comes_first():
    assert await text_or_mood(Mood.Happy) == "enum"
    assert await mood_or_text(Mood.Happy) == "enum"
    # A plain string is a string: identity beats naming a variant.
    assert await text_or_mood("Happy") == "string"
    assert await mood_or_text("Happy") == "string"


async def test_every_element_decides_the_list_type():
    assert await round_trip_list_union([1, "a"]) == [1, "a"]
    assert await round_trip_list_union([1, 2, 3]) == [1, 2, 3]


async def test_a_dict_stands_in_for_a_class():
    assert await round_trip_point({"x": 1, "y": 2}) == Point(x=1, y=2)
    with pytest.raises(TypeError):
        await round_trip_point({"x": 1, "y": 2, "z": 3})  # not a field
    with pytest.raises(TypeError):
        await round_trip_point({"x": 1})  # a field that cannot be null is missing


async def test_a_dict_under_a_class_or_a_map_is_the_map():
    assert await point_or_map({"x": 1, "y": 2}) == "map"
    assert await point_or_map(Point(x=1, y=2)) == "class"


async def test_a_dict_under_two_classes_is_the_one_that_leaves_out_fewest_fields():
    assert await note_or_tag({"text": "t"}) == "tag"


async def test_a_literal_beats_a_conversion():
    assert await float_or_literal(1) == "int"
    assert await float_or_literal(3) == "float"


async def test_an_int_widens_to_float_only_when_exact():
    assert await round_trip_float(2**53) == float(2**53)
    with pytest.raises(TypeError):
        await round_trip_float(2**53 + 1)


async def test_a_member_of_another_enum_is_refused_even_with_the_same_value():
    assert await round_trip_sentiment(Sentiment.Positive) == Sentiment.Positive
    with pytest.raises(TypeError):
        await round_trip_sentiment(Polarity.Positive)
    # A string names a variant.
    assert await round_trip_sentiment("Negative") == Sentiment.Negative


async def test_an_instance_of_a_subclass_is_its_class():
    class Located(Point):
        pass

    assert await round_trip_point(Located(x=1, y=2)) == Point(x=1, y=2)


async def test_argument_errors_come_when_awaited():
    call = round_trip_point()  # nothing raised yet, as with an `async def`
    with pytest.raises(BamlError) as raised:
        await call
    assert raised.value.class_name == "baml.errors.InvalidArgument"
    with pytest.raises(TypeError):
        await round_trip_point(Point(x=1, y=2), 2)  # too many
