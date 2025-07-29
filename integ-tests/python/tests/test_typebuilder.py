from ..baml_client.type_builder import TypeBuilder
import pytest
import baml_py
from baml_py import errors
from typing import List
from ..baml_client import b
from ..baml_client.types import (
    DynInputOutput,
    Hobby,
    Color,
    Person,
    OriginalB,
)
from ..baml_client import partial_types


@pytest.mark.asyncio
async def test_dynamic():
    tb = TypeBuilder()
    tb.Person.add_property("last_name", tb.string().list())
    tb.Person.add_property("height", tb.float().optional()).description(
        "Height in meters"
    )

    tb.Hobby.add_value("chess")
    for name, val in tb.Hobby.list_values():
        val.alias(name.lower())

    tb.Person.add_property("hobbies", tb.Hobby.type().list()).description(
        "Some suggested hobbies they might be good at"
    )

    # no_tb_res = await b.ExtractPeople("My name is Harrison. My hair is black and I'm 6 feet tall.")
    tb_res = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
        {"tb": tb},
    )

    assert len(tb_res) > 0, "Expected non-empty result but got empty."

    for r in tb_res:
        print(r.model_dump())


@pytest.mark.asyncio
async def test_typebuilder_print():
    tb = TypeBuilder()
    tb.Person.add_property("candy", tb.string().list())
    print("Typebuilder print repr: ", tb)
    expected = """TypeBuilder(
  Classes: [
    Person {
      candy string[]
    }
  ]
)"""
    assert str(tb) == expected


@pytest.mark.asyncio
async def test_dynamic_class_output():
    tb = TypeBuilder()
    tb.DynamicOutput.add_property("hair_color", tb.string())
    print(tb.DynamicOutput.list_properties())
    for prop in tb.DynamicOutput.list_properties():
        print(f"Property: {prop}")

    output = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb},
    )
    output = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb},
    )
    print(output.model_dump_json())
    assert output.hair_color == "black"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_class_nested_output_no_stream():
    tb = TypeBuilder()
    nested_class = tb.add_class("Name")
    nested_class.add_property("first_name", tb.string())
    nested_class.add_property("last_name", tb.string().optional())
    nested_class.add_property("middle_name", tb.string().optional())

    other_nested_class = tb.add_class("Address")
    other_nested_class.add_property("street", tb.string())
    other_nested_class.add_property("city", tb.string())
    other_nested_class.add_property("state", tb.string())
    other_nested_class.add_property("zip", tb.string())

    # name should be first in the prompt schema
    tb.DynamicOutput.add_property("name", nested_class.type().optional())
    tb.DynamicOutput.add_property("address", other_nested_class.type().optional())
    tb.DynamicOutput.add_property("hair_color", tb.string()).alias("hairColor")
    tb.DynamicOutput.add_property("height", tb.float().optional())

    output = await b.MyFunc(
        input="My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb},
    )
    print(output.model_dump_json())
    # assert the order of the properties inside output dict:
    assert (
        output.model_dump_json()
        == '{"name":{"first_name":"Mark","last_name":"Gonzalez","middle_name":null},"address":null,"hair_color":"black","height":6.0}'
    )


@pytest.mark.asyncio
async def test_dynamic_class_nested_output_stream():
    tb = TypeBuilder()
    nested_class = tb.add_class("Name")
    nested_class.add_property("first_name", tb.string())
    nested_class.add_property("last_name", tb.string().optional())

    # name should be first in the prompt schema
    tb.DynamicOutput.add_property("name", nested_class.type().optional())
    tb.DynamicOutput.add_property("hair_color", tb.string())

    stream = b.stream.MyFunc(
        input="My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb},
    )
    msgs: List[partial_types.DynamicOutput] = []
    async for msg in stream:
        print("streamed ", msg)
        print("streamed ", msg.model_dump())
        msgs.append(msg)
    output = await stream.get_final_response()

    print(output.model_dump_json())
    # assert the order of the properties inside output dict:
    assert (
        output.model_dump_json()
        == '{"name":{"first_name":"Mark","last_name":"Gonzalez"},"hair_color":"black"}'
    )


@pytest.mark.asyncio
async def test_stream_dynamic_class_output():
    tb = TypeBuilder()
    tb.DynamicOutput.add_property("hair_color", tb.string())
    print(tb.DynamicOutput.list_properties())
    for prop, _ in tb.DynamicOutput.list_properties():
        print(f"Property: {prop}")

    cr = baml_py.ClientRegistry()
    cr.add_llm_client("MyClient", "openai", {"model": "gpt-4o-mini"})
    cr.set_primary("MyClient")
    stream = b.stream.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb, "client_registry": cr},
    )
    msgs: List[partial_types.DynamicOutput] = []
    async for msg in stream:
        print("streamed ", msg.model_dump())
        msgs.append(msg)
    final = await stream.get_final_response()

    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    print("final ", final)
    print("final ", final.model_dump())
    print("final ", final.model_dump_json())
    assert final.hair_color == "black"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_inputs_list2():
    tb = TypeBuilder()
    tb.DynInputOutput.add_property("new_key", tb.string().optional())
    custom_class = tb.add_class("MyBlah")
    custom_class.add_property("nestedKey1", tb.string())
    tb.DynInputOutput.add_property("blah", custom_class.type())

    res = await b.DynamicListInputOutput(
        [
            DynInputOutput.model_validate(
                {
                    "new_key": "hi1",
                    "testKey": "myTest",
                    "blah": {
                        "nestedKey1": "nestedVal",
                    },
                }
            ),
            DynInputOutput.model_validate(
                {
                    "new_key": "hi",
                    "testKey": "myTest",
                    "blah": {
                        "nestedKey1": "nestedVal",
                    },
                }
            ),
        ],
        {"tb": tb},
    )
    assert res[0].new_key == "hi1"  # type: ignore (dynamic property)
    assert res[0].testKey == "myTest"
    assert res[0].blah["nestedKey1"] == "nestedVal"  # type: ignore (dynamic property)
    assert res[1].new_key == "hi"  # type: ignore (dynamic property)
    assert res[1].testKey == "myTest"
    assert res[1].blah["nestedKey1"] == "nestedVal"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_types_new_enum():
    tb = TypeBuilder()
    field_enum = tb.add_enum("Animal")
    animals = ["giraffe", "elephant", "lion"]
    for animal in animals:
        field_enum.add_value(animal.upper())
    tb.Person.add_property("animalLiked", field_enum.type())
    res = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop. I like giraffes.",
        {"tb": tb},
    )
    assert len(res) > 0
    assert res[0].animalLiked == "GIRAFFE"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_types_existing_enum():
    tb = TypeBuilder()
    tb.Hobby.add_value("Golfing")
    res = await b.ExtractHobby(
        "My name is Harrison. My hair is black and I'm 6 feet tall. golf and music are my favorite!.",
        {"tb": tb},
    )
    assert len(res) > 0
    assert "Golfing" in res, res
    assert Hobby.MUSIC in res, res


@pytest.mark.asyncio
async def test_dynamic_literals():
    tb = TypeBuilder()
    animals = tb.union(
        [
            tb.literal_string(animal.upper())
            for animal in ["giraffe", "elephant", "lion"]
        ]
    )
    tb.Person.add_property("animalLiked", animals)
    res = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop. I like giraffes.",
        {"tb": tb},
    )
    assert len(res) > 0
    assert res[0].animalLiked == "GIRAFFE"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_inputs_list():
    tb = TypeBuilder()
    tb.DynInputOutput.add_property("new_key", tb.string().optional())
    custom_class = tb.add_class("MyBlah")
    custom_class.add_property("nestedKey1", tb.string())
    tb.DynInputOutput.add_property("blah", custom_class.type())

    res = await b.DynamicListInputOutput(
        [
            DynInputOutput.model_validate(
                {
                    "new_key": "hi",
                    "testKey": "myTest",
                    "blah": {
                        "nestedKey1": "nestedVal",
                    },
                }
            ),
            DynInputOutput.model_validate(
                {
                    "new_key": "hi",
                    "testKey": "myTest",
                    "blah": {
                        "nestedKey1": "nestedVal",
                    },
                }
            ),
        ],
        {"tb": tb},
    )
    assert res[0].new_key == "hi"  # type: ignore (dynamic property)
    assert res[0].testKey == "myTest"
    assert res[0].blah["nestedKey1"] == "nestedVal"  # type: ignore (dynamic property)
    assert res[1].new_key == "hi"  # type: ignore (dynamic property)
    assert res[1].testKey == "myTest"
    assert res[1].blah["nestedKey1"] == "nestedVal"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_output_map():
    tb = TypeBuilder()
    tb.DynamicOutput.add_property("hair_color", tb.string())
    tb.DynamicOutput.add_property(
        "attributes", tb.map(tb.string(), tb.string())
    ).description("Things like 'eye_color' or 'facial_hair'")
    print(tb.DynamicOutput.list_properties())
    for prop, _ in tb.DynamicOutput.list_properties():
        print(f"Property: {prop}")

    res = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall. I have blue eyes and a beard.",
        baml_options={"tb": tb},
    )

    print("final ", res)
    print("final ", res.model_dump())
    print("final ", res.model_dump_json())
    assert res.hair_color == "black"  # type: ignore (dynamic property)
    assert res.attributes["eye_color"] == "blue"  # type: ignore (dynamic property)
    assert res.attributes["facial_hair"] == "beard"  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_dynamic_output_union():
    tb = TypeBuilder()
    tb.DynamicOutput.add_property("hair_color", tb.string())
    tb.DynamicOutput.add_property(
        "attributes", tb.map(tb.string(), tb.string())
    ).description("Things like 'eye_color' or 'facial_hair'")
    # Define two classes
    class1 = tb.add_class("Class1")
    class1.add_property("meters", tb.float())

    class2 = tb.add_class("Class2")
    class2.add_property("feet", tb.float())
    class2.add_property("inches", tb.float().optional())

    # Use the classes in a union property
    tb.DynamicOutput.add_property("height", tb.union([class1.type(), class2.type()]))
    print(tb.DynamicOutput.list_properties())
    for prop, _ in tb.DynamicOutput.list_properties():
        print(f"Property: {prop}")

    res = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall. I have blue eyes and a beard. I am 30 years old.",
        baml_options={"tb": tb},
    )

    print("final ", res)
    print("final ", res.model_dump())
    print("final ", res.model_dump_json())
    assert res.hair_color == "black"  # type: ignore (dynamic property)
    assert res.attributes["eye_color"] == "blue"  # type: ignore (dynamic property)
    assert res.attributes["facial_hair"] == "beard"  # type: ignore (dynamic property)
    assert res.height["feet"] == 6  # type: ignore (dynamic property)

    res = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 1.8 meters tall. I have blue eyes and a beard. I am 30 years old.",
        baml_options={"tb": tb},
    )

    print("final ", res)
    print("final ", res.model_dump())
    print("final ", res.model_dump_json())
    assert res.hair_color == "black"  # type: ignore (dynamic property)
    assert res.attributes["eye_color"] == "blue"  # type: ignore (dynamic property)
    assert res.attributes["facial_hair"] == "beard"  # type: ignore (dynamic property)
    assert res.height["meters"] == 1.8  # type: ignore (dynamic property)


@pytest.mark.asyncio
async def test_differing_unions():
    tb = TypeBuilder()
    tb.OriginalB.add_property("value2", tb.string())
    res = await b.DifferentiateUnions({"tb": tb})
    assert isinstance(res, OriginalB)


@pytest.mark.asyncio
async def test_add_baml_existing_class():
    tb = TypeBuilder()
    tb.add_baml(
        """
        class ExtraPersonInfo {
            height int
            weight int
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
        }
    """
    )
    res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.",
        {"tb": tb},
    )
    assert res == [
        Person(
            name="John Doe",
            hair_color=Color.YELLOW,
            age=30,  # type: ignore (dynamic property)
            extra={"height": 6, "weight": 180},  # type: ignore (dynamic property)
        )
    ]


@pytest.mark.asyncio
async def test_add_baml_existing_enum():
    tb = TypeBuilder()
    tb.add_baml(
        """
        dynamic enum Hobby {
            VideoGames
            BikeRiding
        }
    """
    )
    res = await b.ExtractHobby("I play video games", {"tb": tb})
    assert res == ["VideoGames"]


@pytest.mark.asyncio
async def test_add_baml_both_classes_and_enums():
    tb = TypeBuilder()
    tb.add_baml(
        """
        class ExtraPersonInfo {
            height int @alias("height_inches")
            weight int @alias("weight_pounds")
        }

        enum Job {
            Programmer
            Architect
            Musician
        }

        dynamic enum Hobby {
            VideoGames
            BikeRiding
        }

        dynamic enum Color {
            BROWN
        }

        dynamic class Person {
            age int?
            extra ExtraPersonInfo?
            job Job?
            hobbies Hobby[]
        }
    """
    )
    res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. My height is 6 feet and I weigh 180 pounds. My hair is brown. I work as a programmer and enjoy bike riding.",
        {"tb": tb},
    )
    assert res == [
        Person(
            name="John Doe",
            hair_color="BROWN",
            age=30,  # type: ignore (dynamic property)
            extra={"height": 72, "weight": 180},  # type: ignore (dynamic property)
            job="Programmer",  # type: ignore (dynamic property)
            hobbies=["BikeRiding"],  # type: ignore (dynamic property)
        )
    ]


@pytest.mark.asyncio
async def test_add_baml_with_attrs():
    tb = TypeBuilder()
    tb.add_baml(
        """
        class ExtraPersonInfo {
            height int @description("In centimeters and rounded to the nearest whole number")
            weight int @description("In kilograms and rounded to the nearest whole number")
        }

        dynamic class Person {
            extra ExtraPersonInfo?
        }
    """
    )
    res = await b.ExtractPeople(
        "My name is John Doe. I'm 30 years old. I'm 6 feet tall and weigh 180 pounds. My hair is yellow.",
        {"tb": tb},
    )
    assert res == [
        Person(
            name="John Doe",
            hair_color=Color.YELLOW,
            extra={"height": 183, "weight": 82},  # type: ignore (dynamic property)
        )
    ]


@pytest.mark.asyncio
async def test_add_baml_error():
    tb = TypeBuilder()
    with pytest.raises(errors.BamlError):
        tb.add_baml(
            """
            dynamic Hobby {
                VideoGames
                BikeRiding
            }
        """
        )


@pytest.mark.asyncio
async def test_add_baml_parser_error():
    tb = TypeBuilder()
    with pytest.raises(errors.BamlError):
        tb.add_baml(
            """
            syntaxerror
        """
        )


@pytest.mark.asyncio
async def test_referencing_existing_class_types():
    tb = TypeBuilder()
    # useful for adding dynamic tools for example
    tb.Person.add_property("props", tb.union([tb.Resume.type(), tb.Hobby.type()]))


def test_typebuilder_and_fieldtype_imports():
    """Test that both TypeBuilder and FieldType can be imported from baml_client.type_builder"""
    # Test importing both from the same module
    from baml_client.type_builder import TypeBuilder, FieldType
    
    # Verify TypeBuilder works
    tb = TypeBuilder()
    assert tb is not None
    assert isinstance(tb, TypeBuilder)
    
    # Verify FieldType is available
    assert FieldType is not None
    
    # Test that TypeBuilder methods return FieldType instances
    string_type = tb.string()
    assert isinstance(string_type, FieldType)

