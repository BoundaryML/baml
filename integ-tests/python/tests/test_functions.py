import uuid
import json
import os
import time
from typing import List, Optional
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64

load_dotenv()
import baml_py
from baml_py import errors

# also test importing the error from the baml_py submodules
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client.globals import (
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
)
from ..baml_client import partial_types
from ..baml_client.types import (
    DynInputOutput,
    Hobby,
    FooAny,
    NamedArgsSingleEnumList,
    NamedArgsSingleClass,
    Nested,
    OriginalB,
    StringToClassEntry,
    MalformedConstraints2,
    LiteralClassHello,
    LiteralClassOne,
    LinkedList,
    Node,
    BinaryNode,
    Tree,
    Forest,
    all_succeeded,
    BlockConstraintForParam,
    NestedBlockConstraintForParam,
    MapKey,
    LinkedListAliasNode,
    ClassToRecAlias,
    NodeWithAliasIndirection,
    MergeAttrs,
    OptionalListAndMap,
    RecursiveAliasDependency,
    JsonEntry,
    SimpleTag,
)
import baml_client.types as types
from ..baml_client.tracing import trace, set_tags, flush, on_log_event
from ..baml_client.type_builder import TypeBuilder
from ..baml_client import reset_baml_env_vars

import datetime
import concurrent.futures
import asyncio
import random


@pytest.mark.asyncio
async def test_env_vars_reset():
    env_vars = {
        "OPENAI_API_KEY": "sk-1234567890",
    }
    reset_baml_env_vars(env_vars)

    @trace
    def top_level_async_tracing():
        reset_baml_env_vars(env_vars)

    @trace
    async def atop_level_async_tracing():
        reset_baml_env_vars(env_vars)

    with pytest.raises(errors.BamlError):
        # Not allowed to call reset_baml_env_vars inside a traced function
        top_level_async_tracing()

    with pytest.raises(errors.BamlError):
        # Not allowed to call reset_baml_env_vars inside a traced function
        await atop_level_async_tracing()

    with pytest.raises(errors.BamlClientHttpError) as excinfo:
        _ = await b.ExtractPeople(
            "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop."
        )
    assert excinfo.value.status_code == 401

    reset_baml_env_vars(os.environ.copy())
    people = await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop."
    )
    assert len(people) > 0


def test_sync():
    res = sync_b.TestFnNamedArgsSingleClass(
        myArg=NamedArgsSingleClass(
            key="key",
            key_two=True,
            key_three=52,
        )
    )
    print("got response", res)
    assert "52" in res


class TestAllInputs:
    @pytest.mark.asyncio
    async def test_single_bool(self):
        res = await b.TestFnNamedArgsSingleBool(True)
        assert res

    @pytest.mark.asyncio
    async def test_single_string_list(self):
        res = await b.TestFnNamedArgsSingleStringList(["a", "b", "c"])
        assert "a" in res and "b" in res and "c" in res

    @pytest.mark.asyncio
    async def test_return_literal_union(self):
        res = await b.LiteralUnionsTest("a")
        assert res == 1 or res == True or res == "string output"

    @pytest.mark.asyncio
    async def test_optional_list_and_map(self):
        res = await b.AllowedOptionals(OptionalListAndMap(p=None, q=None))
        assert res == OptionalListAndMap(p=None, q=None)

        res = await b.AllowedOptionals(OptionalListAndMap(p=["test"], q={"test": "ok"}))
        assert res == OptionalListAndMap(p=["test"], q={"test": "ok"})

    @pytest.mark.asyncio
    async def test_constraints(self):
        res = await b.PredictAge("Greg")
        assert res.certainty.checks["unreasonably_certain"].status == "failed"
        assert not (all_succeeded(res.certainty.checks))

    @pytest.mark.asyncio
    async def test_constraint_union_variant_checking(self):
        res = await b.ExtractContactInfo(
            "Reach me at help@boundaryml.com, or 111-222-3333 if needed."
        )
        assert res.primary.value is not None
        assert res.primary.value == "help@boundaryml.com"
        assert res.secondary.value is not None
        assert res.secondary.value == "111-222-3333"

    @pytest.mark.asyncio
    async def test_return_malformed_constraint(self):
        with pytest.raises(errors.BamlError) as e:
            res = await b.ReturnMalformedConstraints(1)
            assert res.foo.value == 2
            assert res.foo.checks["foo_check"].status == "failed"
        assert "Failed to coerce value" in str(e)

    @pytest.mark.asyncio
    async def test_use_malformed_constraint(self):
        with pytest.raises(errors.BamlError) as e:
            res = await b.UseMalformedConstraints(MalformedConstraints2(foo=2))
            assert res == 3
        assert "object has no method named length" in str(e)

    @pytest.mark.asyncio
    async def test_single_class(self):
        res = await b.TestFnNamedArgsSingleClass(
            myArg=NamedArgsSingleClass(
                key="key",
                key_two=True,
                key_three=52,
            )
        )
        assert "52" in res

    @pytest.mark.asyncio
    async def test_multiple_args(self):
        res = await b.TestMulticlassNamedArgs(
            myArg=NamedArgsSingleClass(
                key="key",
                key_two=True,
                key_three=52,
            ),
            myArg2=NamedArgsSingleClass(
                key="key",
                key_two=True,
                key_three=64,
            ),
        )
        assert "52" in res and "64" in res

    @pytest.mark.asyncio
    async def test_single_enum_list(self):
        res = await b.TestFnNamedArgsSingleEnumList([NamedArgsSingleEnumList.TWO])
        assert "TWO" in res

    @pytest.mark.asyncio
    async def test_single_float(self):
        res = await b.TestFnNamedArgsSingleFloat(3.12)
        assert "3.12" in res

    @pytest.mark.asyncio
    async def test_single_int(self):
        res = await b.TestFnNamedArgsSingleInt(3566)
        assert "3566" in res

    @pytest.mark.asyncio
    async def test_single_literal_int(self):
        res = await b.TestNamedArgsLiteralInt(1)
        assert "1" in res

    @pytest.mark.asyncio
    async def test_single_literal_bool(self):
        res = await b.TestNamedArgsLiteralBool(True)
        assert "true" in res

    @pytest.mark.asyncio
    async def test_single_literal_string(self):
        res = await b.TestNamedArgsLiteralString("My String")
        assert "My String" in res

    @pytest.mark.asyncio
    async def test_class_with_literal_prop(self):
        res = await b.FnLiteralClassInputOutput(input=LiteralClassHello(prop="hello"))
        assert isinstance(res, LiteralClassHello)

    @pytest.mark.asyncio
    async def test_literal_classs_with_literal_union_prop(self):
        res = await b.FnLiteralUnionClassInputOutput(input=LiteralClassOne(prop="one"))
        assert isinstance(res, LiteralClassOne)

    @pytest.mark.asyncio
    async def test_single_map_string_to_string(self):
        res = await b.TestFnNamedArgsSingleMapStringToString(
            {"lorem": "ipsum", "dolor": "sit"}
        )
        assert "lorem" in res

    @pytest.mark.asyncio
    async def test_single_map_string_to_class(self):
        res = await b.TestFnNamedArgsSingleMapStringToClass(
            {"lorem": StringToClassEntry(word="ipsum")}
        )
        assert res["lorem"].word == "ipsum"

    @pytest.mark.asyncio
    async def test_single_map_string_to_map(self):
        res = await b.TestFnNamedArgsSingleMapStringToMap({"lorem": {"word": "ipsum"}})
        assert res["lorem"]["word"] == "ipsum"

    @pytest.mark.asyncio
    async def test_enum_key_in_map(self):
        res = await b.InOutEnumMapKey({MapKey.A: "A"}, {MapKey.B: "B"})
        assert res[MapKey.A] == "A"
        assert res[MapKey.B] == "B"

    @pytest.mark.asyncio
    async def test_literal_string_union_key_in_map(self):
        res = await b.InOutLiteralStringUnionMapKey({"one": "1"}, {"two": "2"})
        assert res["one"] == "1"
        assert res["two"] == "2"

    @pytest.mark.asyncio
    async def test_single_literal_string_key_in_map(self):
        res = await b.InOutSingleLiteralStringMapKey({"key": "1"})
        assert res["key"] == "1"

    @pytest.mark.asyncio
    async def test_primitive_union_alias(self):
        res = await b.PrimitiveAlias("test")
        assert res == "test"

    @pytest.mark.asyncio
    async def test_map_alias(self):
        res = await b.MapAlias({"A": ["B", "C"], "B": [], "C": []})
        assert res == {"A": ["B", "C"], "B": [], "C": []}

    @pytest.mark.asyncio
    async def test_alias_union(self):
        res = await b.NestedAlias("test")
        assert res == "test"

        res = await b.NestedAlias({"A": ["B", "C"], "B": [], "C": []})
        assert res == {"A": ["B", "C"], "B": [], "C": []}

    @pytest.mark.asyncio
    async def test_alias_pointing_to_recursive_class(self):
        res = await b.AliasThatPointsToRecursiveType(
            LinkedListAliasNode(value=1, next=None)
        )
        assert res == LinkedListAliasNode(value=1, next=None)

    @pytest.mark.asyncio
    async def test_class_pointing_to_alias_that_points_to_recursive_class(self):
        res = await b.ClassThatPointsToRecursiveClassThroughAlias(
            ClassToRecAlias(list=LinkedListAliasNode(value=1, next=None))
        )
        assert res == ClassToRecAlias(list=LinkedListAliasNode(value=1, next=None))

    @pytest.mark.asyncio
    async def test_recursive_class_with_alias_indirection(self):
        res = await b.RecursiveClassWithAliasIndirection(
            NodeWithAliasIndirection(
                value=1, next=NodeWithAliasIndirection(value=2, next=None)
            )
        )
        assert res == NodeWithAliasIndirection(
            value=1, next=NodeWithAliasIndirection(value=2, next=None)
        )

    @pytest.mark.asyncio
    async def test_merge_alias_attributes(self):
        res = await b.MergeAliasAttributes(123)
        assert res.amount.value == 123
        assert res.amount.checks["gt_ten"].status == "succeeded"

    @pytest.mark.asyncio
    async def test_return_alias_with_merged_attrs(self):
        res = await b.ReturnAliasWithMergedAttributes(123)
        assert res.value == 123
        assert res.checks["gt_ten"].status == "succeeded"

    @pytest.mark.asyncio
    async def test_alias_with_multiple_attrs(self):
        res = await b.AliasWithMultipleAttrs(123)
        assert res.value == 123
        assert res.checks["gt_ten"].status == "succeeded"

    @pytest.mark.asyncio
    async def test_simple_recursive_map_alias(self):
        res = await b.SimpleRecursiveMapAlias({"one": {"two": {"three": {}}}})
        assert res == {"one": {"two": {"three": {}}}}

    @pytest.mark.asyncio
    async def test_simple_recursive_list_alias(self):
        res = await b.SimpleRecursiveListAlias([[], [], [[]]])
        assert res == [[], [], [[]]]

    @pytest.mark.asyncio
    async def test_recursive_alias_cycles(self):
        res = await b.RecursiveAliasCycle([[], [], [[]]])
        assert res == [[], [], [[]]]

    @pytest.mark.asyncio
    async def test_json_type_alias_cycle(self):
        data = {
            "number": 1,
            "string": "test",
            "bool": True,
            "list": [1, 2, 3],
            "object": {"number": 1, "string": "test", "bool": True, "list": [1, 2, 3]},
            "json": {
                "number": 1,
                "string": "test",
                "bool": True,
                "list": [1, 2, 3],
                "object": {
                    "number": 1,
                    "string": "test",
                    "bool": True,
                    "list": [1, 2, 3],
                },
            },
        }

        res = await b.JsonTypeAliasCycle(data)
        assert res == data
        assert res["json"]["object"]["list"] == [1, 2, 3]

    # TODO. Doesn't work because of Pydantic bug
    # https://github.com/pydantic/pydantic/issues/2279#issuecomment-1876108310
    # https://github.com/pydantic/pydantic/issues/11320
    #
    # @pytest.mark.asyncio
    # async def test_json_type_alias_as_class_dependency(self):
    #     data = {
    #         "number": 1,
    #         "string": "test",
    #         "bool": True,
    #         "list": [1, 2, 3],
    #         "object": {"number": 1, "string": "test", "bool": True, "list": [1, 2, 3]},
    #         "json": {
    #             "number": 1,
    #             "string": "test",
    #             "bool": True,
    #             "list": [1, 2, 3],
    #             "object": {
    #                 "number": 1,
    #                 "string": "test",
    #                 "bool": True,
    #                 "list": [1, 2, 3],
    #             },
    #         },
    #     }
    #
    #     res = await b.TakeRecAliasDep(RecursiveAliasDependency(value=data))
    #     assert res == RecursiveAliasDependency(value=data)
    #     assert res.value["json"]["object"]["list"] == [1, 2, 3]


    @pytest.mark.asyncio
    async def test_union_of_recursive_alias_or_class(self):
        res = await b.ReturnJsonEntry(json.dumps({"a": "A", "b": {"c": "C"}}, indent=4))
        assert res == {"a": SimpleTag(field="A"), "b": {"c": SimpleTag(field="C")}}


class MyCustomClass(NamedArgsSingleClass):
    date: datetime.datetime


@pytest.mark.asyncio
async def accepts_subclass_of_baml_type():
    print("calling with class")
    _ = await b.TestFnNamedArgsSingleClass(
        myArg=MyCustomClass(
            key="key", key_two=True, key_three=52, date=datetime.datetime.now()
        )
    )


@pytest.mark.asyncio
async def test_should_work_for_all_outputs():
    a = "a"  # dummy
    res = await b.FnOutputBool(a)
    assert res == True

    integer = await b.FnOutputInt(a)
    assert integer == 5

    literal_integer = await b.FnOutputLiteralInt(a)
    assert literal_integer == 5

    literal_bool = await b.FnOutputLiteralBool(a)
    assert literal_bool == False

    literal_string = await b.FnOutputLiteralString(a)
    assert literal_string == "example output"

    list = await b.FnOutputClassList(a) # Broken
    assert len(list) > 0
    assert len(list[0].prop1) > 0

    classWEnum = await b.FnOutputClassWithEnum(a)
    assert classWEnum.prop2 in ["ONE", "TWO"]

    classs = await b.FnOutputClass(a)
    assert classs.prop1 is not None
    assert classs.prop2 == 540

    enumList = await b.FnEnumListOutput(a)
    assert len(enumList) == 2

    myEnum = await b.FnEnumOutput(a)
    # As no check is added for myEnum, adding a simple assert to ensure the call was made
    assert myEnum is not None


@pytest.mark.asyncio
async def test_should_work_with_image_url():
    res = await b.TestImageInput(
        img=baml_py.Image.from_url(
            "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
        )
    )
    assert_that(res.lower()).matches(r"(green|yellow|shrek|ogre)")


@pytest.mark.asyncio
async def test_should_work_with_image_list():
    res = await b.TestImageListInput(
        imgs=[
            baml_py.Image.from_url(
                "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
            ),
            baml_py.Image.from_url(
                "https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png"
            ),
        ]
    )
    assert_that(res.lower()).matches(r"(green|yellow)")


@pytest.mark.asyncio
async def test_should_work_with_vertex():
    res = await b.TestVertex("donkey kong")
    assert_that("donkey kong" in res.lower())


@pytest.mark.asyncio
async def test_should_work_with_image_base64():
    res = await b.TestImageInput(img=baml_py.Image.from_base64("image/png", image_b64))
    assert_that(res.lower()).matches(r"(green|yellow|shrek|ogre)")


@pytest.mark.asyncio
async def test_should_work_with_audio_base64():
    res = await b.AudioInput(aud=baml_py.Audio.from_base64("audio/mp3", audio_b64))
    assert "yes" in res.lower()


@pytest.mark.asyncio
async def test_should_work_with_audio_url():
    res = await b.AudioInput(
        aud=baml_py.Audio.from_url(
            "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg"
        )
    )
    assert "no" in res.lower()


@pytest.mark.asyncio
async def test_works_with_retries2():
    try:
        await b.TestRetryExponential()
        assert False, "Expected an exception but none was raised."
    except Exception as e:
        print("Expected error", e)


@pytest.mark.asyncio
async def test_works_with_fallbacks():
    res = await b.TestFallbackClient()
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_works_with_failing_azure_fallback():
    with pytest.raises(errors.BamlClientError) as e:
        _ = await b.TestSingleFallbackClient()
    assert "ConnectError" in str(e.value)


@pytest.mark.asyncio
async def test_claude():
    res = await b.PromptTestClaude(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_gemini():
    geminiRes = await b.TestGemini(input="Dr. Pepper")
    print(f"LLM output from Gemini: {geminiRes}")
    assert len(geminiRes) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_gemini_system_prompt():
    geminiRes = await b.TestGeminiSystem(input="Dr. Pepper")
    print(f"LLM output from Gemini: {geminiRes}")
    assert len(geminiRes) > 0, "Expected non-empty result but got empty."

@pytest.mark.asyncio
async def test_gemini_system_prompt_as_chat():
    geminiRes = await b.TestGeminiSystemAsChat(input="Dr. Pepper")
    print(f"LLM output from Gemini: {geminiRes}")
    assert len(geminiRes) > 0, "Expected non-empty result but got empty."

@pytest.mark.asyncio
async def test_gemini_streaming():
    geminiRes = await b.stream.TestGemini(input="Dr. Pepper").get_final_response()
    print(f"LLM output from Gemini: {geminiRes}")

    assert len(geminiRes) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_gemini_openai_generic_system_prompt():
    res = await b.TestGeminiOpenAiGeneric()
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_aws():
    res = await b.TestAws(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_openai_shorthand():
    res = await b.TestOpenAIShorthand(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_openai_shorthand_streaming():
    res = await b.stream.TestOpenAIShorthand(
        input="Mt Rainier is tall"
    ).get_final_response()
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_anthropic_shorthand():
    res = await b.TestAnthropicShorthand(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_anthropic_shorthand_streaming():
    res = await b.stream.TestAnthropicShorthand(
        input="Mt Rainier is tall"
    ).get_final_response()
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_fallback_to_shorthand():
    res = await b.TestFallbackToShorthand(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_aws_streaming():
    res = await b.stream.TestAws(input="Mt Rainier is tall").get_final_response()
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_streaming():
    stream = b.stream.PromptTestStreaming(
        input="Programming languages are fun to create"
    )
    msgs: list[str] = []

    start_time = asyncio.get_event_loop().time()
    last_msg_time = start_time
    first_msg_time = start_time + 10
    async for msg in stream:
        msgs.append(str(msg))
        if len(msgs) == 1:
            first_msg_time = asyncio.get_event_loop().time()

        last_msg_time = asyncio.get_event_loop().time()

    final = await stream.get_final_response()

    assert (
        first_msg_time - start_time <= 1.5
    ), "Expected first message within 1 second but it took longer."
    assert (
        last_msg_time - start_time >= 1
    ), "Expected last message after 1.5 seconds but it was earlier."
    assert len(final) > 0, "Expected non-empty final but got empty."
    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    for prev_msg, msg in zip(msgs, msgs[1:]):
        assert msg.startswith(prev_msg), (
            "Expected messages to be continuous, but prev was %r and next was %r"
            % (
                prev_msg,
                msg,
            )
        )
    assert msgs[-1] == final, "Expected last stream message to match final response."


@pytest.mark.asyncio
async def test_streaming_uniterated():
    final = await b.stream.PromptTestStreaming(
        input="The color blue makes me sad"
    ).get_final_response()
    assert len(final) > 0, "Expected non-empty final but got empty."


def test_streaming_sync():
    stream = sync_b.stream.PromptTestStreaming(
        input="Programming languages are fun to create"
    )
    msgs: list[str] = []

    start_time = asyncio.get_event_loop().time()
    last_msg_time = start_time
    first_msg_time = start_time + 10
    for msg in stream:
        msgs.append(str(msg))
        if len(msgs) == 1:
            first_msg_time = asyncio.get_event_loop().time()

        last_msg_time = asyncio.get_event_loop().time()

    final = stream.get_final_response()

    assert (
        first_msg_time - start_time <= 1.5
    ), "Expected first message within 1 second but it took longer."
    assert (
        last_msg_time - start_time >= 1
    ), "Expected last message after 1.5 seconds but it was earlier."
    assert len(final) > 0, "Expected non-empty final but got empty."
    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    for prev_msg, msg in zip(msgs, msgs[1:]):
        assert msg.startswith(prev_msg), (
            "Expected messages to be continuous, but prev was %r and next was %r"
            % (
                prev_msg,
                msg,
            )
        )
    assert msgs[-1] == final, "Expected last stream message to match final response."


def test_streaming_uniterated_sync():
    final = sync_b.stream.PromptTestStreaming(
        input="The color blue makes me sad"
    ).get_final_response()
    assert len(final) > 0, "Expected non-empty final but got empty."


@pytest.mark.asyncio
async def test_streaming_claude():
    stream = b.stream.PromptTestClaude(input="Mt Rainier is tall")
    msgs: list[str] = []
    async for msg in stream:
        msgs.append(str(msg))
    final = await stream.get_final_response()

    assert len(final) > 0, "Expected non-empty final but got empty."
    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    for prev_msg, msg in zip(msgs, msgs[1:]):
        assert msg.startswith(prev_msg), (
            "Expected messages to be continuous, but prev was %r and next was %r"
            % (
                prev_msg,
                msg,
            )
        )
    print("msgs:")
    print(msgs[-1])
    print("final:")
    print(final)
    assert msgs[-1] == final, "Expected last stream message to match final response."


@pytest.mark.asyncio
async def test_streaming_gemini():
    stream = b.stream.TestGemini(input="Dr.Pepper")
    msgs: list[str] = []
    async for msg in stream:
        if msg is not None:
            msgs.append(msg)
    final = await stream.get_final_response()

    assert len(final) > 0, "Expected non-empty final but got empty."
    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    for prev_msg, msg in zip(msgs, msgs[1:]):
        assert msg.startswith(prev_msg), (
            "Expected messages to be continuous, but prev was %r and next was %r"
            % (
                prev_msg,
                msg,
            )
        )
    print("msgs:")
    print(msgs[-1])
    print("final:")
    print(final)
    assert msgs[-1] == final, "Expected last stream message to match final response."


@pytest.mark.asyncio
async def test_tracing_async_only():
    @trace
    async def top_level_async_tracing():
        @trace
        async def nested_dummy_fn(_foo: str):
            time.sleep(0.5 + random.random())
            return "nested dummy fn"

        @trace
        async def dummy_fn(foo: str):
            await asyncio.gather(
                b.FnOutputClass(foo),
                nested_dummy_fn(foo),
            )
            return "dummy fn"

        await asyncio.gather(
            dummy_fn("dummy arg 1"),
            dummy_fn("dummy arg 2"),
            dummy_fn("dummy arg 3"),
        )
        await asyncio.gather(
            parent_async("first-arg-value"), parent_async2("second-arg-value")
        )
        return 1

    # Clear any existing traces
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
    _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

    res = await top_level_async_tracing()
    assert_that(res).is_equal_to(1)

    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
    stats = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()
    print("STATS", stats)
    assert_that(stats.started).is_equal_to(15)
    assert_that(stats.finalized).is_equal_to(stats.started)
    assert_that(stats.submitted).is_equal_to(stats.started)
    assert_that(stats.sent).is_equal_to(stats.started)
    assert_that(stats.done).is_equal_to(stats.started)
    assert_that(stats.failed).is_equal_to(0)


def test_tracing_sync():
    # res = parent_sync("first-arg-value")
    _ = sync_dummy_func("second-dummycall-arg")


def test_tracing_thread_pool():
    trace_thread_pool()


@pytest.mark.asyncio
async def test_tracing_thread_pool_async():
    await trace_thread_pool_async()


@pytest.mark.asyncio
async def test_tracing_async_gather():
    await trace_async_gather()


@pytest.mark.asyncio
async def test_tracing_async_gather_top_level():
    await asyncio.gather(*[async_dummy_func("second-dummycall-arg") for _ in range(10)])


@trace
def trace_thread_pool():
    with concurrent.futures.ThreadPoolExecutor() as executor:
        # Create 10 tasks and execute them
        futures = [
            executor.submit(parent_sync, "second-dummycall-arg") for _ in range(10)
        ]
        for future in concurrent.futures.as_completed(futures):
            future.result()


@trace
async def trace_thread_pool_async():
    with concurrent.futures.ThreadPoolExecutor() as executor:
        # Create 10 tasks and execute them
        futures = [executor.submit(trace_async_gather) for _ in range(10)]
        for future in concurrent.futures.as_completed(futures):
            _ = await future.result()


@trace
async def trace_async_gather():
    await asyncio.gather(
        *[async_dummy_func("handcrafted-artisan-arg") for _ in range(10)]
    )


@trace
async def parent_async(myStr: str):
    set_tags(myKey="myVal")
    await async_dummy_func(myStr)
    await b.FnOutputClass(myStr)
    sync_dummy_func(myStr)
    return "hello world parentasync"


@trace
async def parent_async2(myStr: str):
    return "hello world parentasync2"


@trace
def parent_sync(myStr: str):
    import time
    import random

    time.sleep(0.5 + random.random())
    sync_dummy_func(myStr)
    return "hello world parentsync"


@trace
async def async_dummy_func(myArgggg: str):
    await asyncio.sleep(0.5 + random.random())
    return "asyncDummyFuncOutput"


@trace
def sync_dummy_func(dummyFuncArg: str):
    return "pythonDummyFuncOutput"


@pytest.fixture(scope="session", autouse=True)
def cleanup():
    """Cleanup a testing directory once we are finished."""
    flush()


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
    assert res[0].animalLiked == "GIRAFFE", res[0]


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
    assert res[0].animalLiked == "GIRAFFE"


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
async def test_nested_class_streaming():
    stream = b.stream.FnOutputClassNested(
        input="My name is Harrison. My hair is black and I'm 6 feet tall."
    )
    msgs: List[partial_types.TestClassNested] = []
    async for msg in stream:
        print("streamed ", msg.model_dump(mode="json"))
        msgs.append(msg)
    final = await stream.get_final_response()

    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    print("final ", final.model_dump(mode="json"))


@pytest.mark.asyncio
async def test_dynamic_client_with_openai():
    cb = baml_py.ClientRegistry()
    cb.add_llm_client("MyClient", "openai", {"model": "gpt-3.5-turbo"})
    cb.set_primary("MyClient")

    capitol = await b.ExpectFailure(
        baml_options={"client_registry": cb},
    )
    assert_that(capitol.lower()).contains("london")


@pytest.mark.asyncio
async def test_dynamic_client_with_vertex_json_str_creds():
    cb = baml_py.ClientRegistry()
    cb.add_llm_client(
        "MyClient",
        "vertex-ai",
        {
            "model": "gemini-1.5-pro",
            "location": "us-central1",
            "credentials": os.environ[
                "INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT"
            ],
        },
    )
    cb.set_primary("MyClient")

    capitol = await b.ExpectFailure(
        baml_options={"client_registry": cb},
    )
    assert_that(capitol.lower()).contains("london")


@pytest.mark.asyncio
async def test_dynamic_client_with_vertex_json_object_creds():
    cb = baml_py.ClientRegistry()
    cb.add_llm_client(
        "MyClient",
        "vertex-ai",
        {
            "model": "gemini-1.5-pro",
            "location": "us-central1",
            "credentials": json.loads(
                os.environ["INTEG_TESTS_GOOGLE_APPLICATION_CREDENTIALS_CONTENT"]
            ),
        },
    )
    cb.set_primary("MyClient")

    capitol = await b.ExpectFailure(
        baml_options={"client_registry": cb},
    )
    assert_that(capitol.lower()).contains("london")


@pytest.mark.asyncio
async def test_event_log_hook():
    def event_log_hook(event: baml_py.baml_py.BamlLogEvent):
        print("Event log hook1: ")
        print("Event log event ", event)

    flush()  # clear any existing hooks
    on_log_event(event_log_hook)
    res = await b.TestFnNamedArgsSingleStringList(["a", "b", "c"])
    assert res
    flush()  # clear the hook
    on_log_event(None)


@pytest.mark.asyncio
async def test_aws_bedrock():
    ## unstreamed
    res = await b.TestAws("lightning in a rock")
    print("unstreamed", res)

    ## streamed
    stream = b.stream.TestAws("lightning in a rock")

    async for msg in stream:
        if msg:
            print("streamed ", repr(msg[-100:]))

    res = await stream.get_final_response()
    print("streamed final", res)
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_aws_bedrock_invalid_region():
    ## unstreamed
    with pytest.raises(errors.BamlClientError) as excinfo:
        res = await b.TestAwsInvalidRegion("lightning in a rock")
        print("unstreamed", res)

    assert "DispatchFailure" in str(excinfo)


@pytest.mark.asyncio
async def test_serialization_exception():
    with pytest.raises(Exception) as excinfo:
        await b.DummyOutputFunction("dummy input")

    print("Exception message from test: ", excinfo)
    assert "Failed to coerce" in str(excinfo)


@pytest.mark.asyncio
async def test_stream_serialization_exception():
    with pytest.raises(Exception) as excinfo:
        stream = b.stream.DummyOutputFunction("dummy input")
        async for msg in stream:
            print("streamed ", msg)

        _ = await stream.get_final_response()

    print("Exception message: ", excinfo)
    assert "Failed to coerce" in str(excinfo)


def test_stream2_serialization_exception():
    tb = TypeBuilder()
    tb.DummyOutput.add_property("nonce3", tb.string())

    async def stream_func():
        with pytest.raises(Exception) as excinfo:
            stream = b.stream.DummyOutputFunction("dummy input", {"tb": tb})
            async for msg in stream:
                print("streamed ", msg)

            _ = await stream.get_final_response()

        print("Exception message: ", excinfo)
        assert "Failed to coerce" in str(excinfo)

    asyncio.run(stream_func())


@pytest.mark.asyncio
async def test_descriptions():
    res = await b.SchemaDescriptions(
        "donkey kong"
    )  # Assuming this returns a Pydantic model

    # Check Schema values
    assert res.prop1 == "one"

    # Check Nested values
    assert isinstance(res.prop2, Nested)
    assert res.prop2.prop3 == "three"
    assert res.prop2.prop4 == "four"

    # Check Nested2 values
    assert not isinstance(res.prop2, str)
    assert res.prop2.prop20.prop11 == "three"
    assert res.prop2.prop20.prop12 == "four"

    assert res.prop5 == ["hi"]  # Assuming it's a list with one item
    assert res.prop6 == "blah"
    assert res.nested_attrs == ["nested"]  # Assuming it's a list with one item
    assert res.parens == "parens1"
    assert res.other_group == "other"


@pytest.mark.asyncio
async def test_caching():
    story_idea = f"""
In a futuristic world where dreams are a marketable asset and collective experience, an introverted and socially inept teenager named Alex realizes they have a unique and potent skill to not only observe but also alter the dreams of others. Initially excited by this newfound talent, Alex starts discreetly modifying the dreams of peers and relatives, aiding them in conquering fears, boosting self-esteem, or embarking on fantastical journeys. As Alex's abilities expand, so does their sway. They begin marketing exclusive dream experiences on the underground market, designing complex and captivating dreamscapes for affluent clients. However, the boundary between dream and reality starts to fade for those subjected to Alex's creations. Some clients find it difficult to distinguish between their genuine memories and the fabricated ones inserted by Alex's dream manipulation.

Challenges emerge when a secretive government organization becomes aware of Alex's distinct talents. They propose Alex utilize their gift for "the greater good," suggesting uses in therapy, criminal reform, and even national defense. Concurrently, a covert resistance group contacts Alex, cautioning them about the risks of dream manipulation and the potential for widespread control and exploitation. Trapped between these conflicting forces, Alex must navigate a tangled web of moral dilemmas. They wrestle with issues of free will, the essence of consciousness, and the duty that comes with having influence over people's minds. As the repercussions of their actions ripple outward, impacting the lives of loved ones and strangers alike, Alex is compelled to face the true nature of their power and decide how—or if—it should be wielded.

The narrative investigates themes of identity, the subconscious, the ethics of technology, and the power of creativity. It explores the possible outcomes of a world where our most intimate thoughts and experiences are no longer truly our own, and scrutinizes the fine line between aiding others and manipulating them for personal benefit or a perceived greater good. The story further delves into the societal ramifications of such abilities, questioning the moral limits of altering consciousness and the potential for misuse in a world where dreams can be commercialized. It challenges the reader to contemplate the impact of technology on personal freedom and the ethical duties of those who wield such power.

As Alex's journey progresses, they meet various individuals whose lives have been influenced by their dream manipulations, each offering a distinct viewpoint on the ethical issues at hand. From a peer who gains newfound confidence to a wealthy client who becomes dependent on the dreamscapes, the ripple effects of Alex's actions are significant and extensive. The government agency's interest in Alex's abilities raises questions about the potential for state control and surveillance, while the resistance movement underscores the dangers of unchecked power and the necessity of protecting individual freedoms.

Ultimately, Alex's story is one of self-discovery and moral reflection, as they must choose whether to use their abilities for personal gain, align with the government's vision of a controlled utopia, or join the resistance in their struggle for freedom and autonomy. The narrative encourages readers to reflect on the nature of reality, the boundaries of human experience, and the ethical implications of a world where dreams are no longer private sanctuaries but shared and manipulated commodities. It also examines the psychological impact on Alex, who must cope with the burden of knowing the intimate fears and desires of others, and the isolation that comes from being unable to share their own dreams without altering them.

The story further investigates the technological progress that has made dream manipulation feasible, questioning the role of innovation in society and the potential for both advancement and peril. It considers the societal divide between those who can afford to purchase enhanced dream experiences and those who cannot, highlighting issues of inequality and access. As Alex becomes more ensnared in the web of their own making, they must confront the possibility that their actions could lead to unintended consequences, not just for themselves but for the fabric of society as a whole.

In the end, Alex's journey is a cautionary tale about the power of dreams and the responsibilities that come with wielding such influence. It serves as a reminder of the importance of ethical considerations in the face of technological advancement and the need to balance innovation with humanity. The story leaves readers pondering the true cost of a world where dreams are no longer sacred, and the potential for both wonder and danger in the uncharted territories of the mind. But it's also a story about the power of imagination and the potential for change, even in a world where our deepest thoughts are no longer our own. And it's a story about the power of choice, and the importance of fighting for the freedom to dream.

In conclusion, this story is a reflection on the power of dreams and the responsibilities that come with wielding such influence. It serves as a reminder of the importance of ethical considerations in the face of technological advancement and the need to balance innovation with humanity. The story leaves readers pondering the true cost of a world where dreams are no longer sacred, and the potential for both wonder and danger in the uncharted territories of the mind. But it's also a story about the power of imagination and the potential for change, even in a world where our deepest thoughts are no longer our own. And it's a story about the power of choice, and the importance of fighting for the freedom to dream.
"""
    rand = uuid.uuid4().hex
    story_idea = rand + story_idea

    start = time.time()
    _ = await b.TestCaching(story_idea, "1. try to be funny")
    duration = time.time() - start

    start = time.time()
    _ = await b.TestCaching(story_idea, "1. try to be funny")
    duration2 = time.time() - start

    print("Duration no caching: ", duration)
    print("Duration with caching: ", duration2)

    assert (
        duration2 < duration
    ), f"{duration2} < {duration}. Expected second call to be faster than first by a large margin."


@pytest.mark.asyncio
async def test_arg_exceptions():
    with pytest.raises(IndexError):
        print("this should fail:", [0, 1, 2][5])

    with pytest.raises(errors.BamlInvalidArgumentError):
        _ = await b.TestCaching(
            111,  # type: ignore -- intentionally passing an int instead of a string
            "..",
        )

    with pytest.raises(errors.BamlClientError):
        cr = baml_py.ClientRegistry()
        cr.add_llm_client(
            "MyClient", "openai", {"model": "gpt-4o-mini", "api_key": "INVALID_KEY"}
        )
        cr.set_primary("MyClient")
        await b.MyFunc(
            input="My name is Harrison. My hair is black and I'm 6 feet tall.",
            baml_options={"client_registry": cr},
        )

    with pytest.raises(errors.BamlClientHttpError) as excinfo:
        cr = baml_py.ClientRegistry()
        cr.add_llm_client(
            "MyClient", "openai", {"model": "gpt-4o-mini", "api_key": "INVALID_KEY"}
        )
        cr.set_primary("MyClient")
        await b.MyFunc(
            input="My name is Harrison. My hair is black and I'm 6 feet tall.",
            baml_options={"client_registry": cr},
        )
    assert excinfo.value.status_code == 401

    # test missing model
    with pytest.raises(errors.BamlClientHttpError) as excinfo:
        cr = baml_py.ClientRegistry()
        cr.add_llm_client(
            "MyClient", "openai", {"model": "random-model"}
        )
        cr.set_primary("MyClient")
        await b.MyFunc(
            input="My name is Harrison. My hair is black and I'm 6 feet tall.",
            baml_options={"client_registry": cr},
        )
    assert excinfo.value.status_code == 404


    with pytest.raises(errors.BamlValidationError):
        await b.DummyOutputFunction("dummy input")


@pytest.mark.asyncio
async def test_map_as_param():
    with pytest.raises(errors.BamlInvalidArgumentError):
        _ = await b.TestFnNamedArgsSingleMapStringToMap(
            {"a": "b"}
        )  # intentionally passing the wrong type


@pytest.mark.asyncio
async def test_baml_validation_error_format():
    with pytest.raises(errors.BamlValidationError) as excinfo:
        try:
            await b.DummyOutputFunction("blah")
        except errors.BamlValidationError as e:
            print("Error: ", e)
            assert hasattr(e, "prompt"), "Error object should have 'prompt' attribute"
            assert hasattr(
                e, "raw_output"
            ), "Error object should have 'raw_output' attribute"
            assert hasattr(e, "message"), "Error object should have 'message' attribute"
            assert 'Say "hello there"' in e.prompt

            raise e
    assert "Failed to parse" in str(excinfo)


@pytest.mark.asyncio
async def test_no_stream_big_integer():
    stream = b.stream.StreamOneBigNumber(digits=12)
    msgs: List[int | None] = []
    async for msg in stream:
        msgs.append(msg)
    print("msgs:")
    print(msgs)
    res = await stream.get_final_response()
    print("res:")
    print(res)
    for msg in msgs:
        assert True if msg is None else msg == res


@pytest.mark.asyncio
async def test_no_stream_object_with_numbers():
    stream = b.stream.StreamBigNumbers(digits=12)
    msgs: List[partial_types.BigNumbers] = []
    async for msg in stream:
        msgs.append(msg)
    res = await stream.get_final_response()

    # If Numbers aren't being streamed, then for every message, the partial
    # field should either be None, or exactly the value in the final result.
    for msg in msgs:
        assert True if msg.a is None else msg.a == res.a
        assert True if msg.b is None else msg.b == res.b


@pytest.mark.asyncio
async def test_no_stream_compound_object():
    stream = b.stream.StreamingCompoundNumbers(digits=12, yapping=False)
    msgs: List[partial_types.CompoundBigNumbers] = []
    async for msg in stream:
        msgs.append(msg)
    res = await stream.get_final_response()
    for msg in msgs:
        if msg.big is not None:
            assert True if msg.big.a is None else msg.big.a == res.big.a
            assert True if msg.big.b is None else msg.big.b == res.big.b
        for msgEntry, resEntry in zip(msg.big_nums, res.big_nums):
            assert True if msgEntry.a is None else msgEntry.a == resEntry.a
            assert True if msgEntry.b is None else msgEntry.b == resEntry.b
        if msg.another is not None:
            assert True if msg.another.a is None else msg.another.a == res.another.a
            assert True if msg.another.b is None else msg.another.b == res.another.b


@pytest.mark.asyncio
async def test_no_stream_compound_object_with_yapping():
    stream = b.stream.StreamingCompoundNumbers(digits=12, yapping=True)
    msgs: List[partial_types.CompoundBigNumbers] = []
    async for msg in stream:
        msgs.append(msg)
    res = await stream.get_final_response()
    for msg in msgs:
        if msg.big is not None:
            assert True if msg.big.a is None else msg.big.a == res.big.a
            assert True if msg.big.b is None else msg.big.b == res.big.b
        for msgEntry, resEntry in zip(msg.big_nums, res.big_nums):
            assert True if msgEntry.a is None else msgEntry.a == resEntry.a
            assert True if msgEntry.b is None else msgEntry.b == resEntry.b
        if msg.another is not None:
            assert True if msg.another.a is None else msg.another.a == res.another.a
            assert True if msg.another.b is None else msg.another.b == res.another.b


@pytest.mark.asyncio
async def test_differing_unions():
    tb = TypeBuilder()
    tb.OriginalB.add_property("value2", tb.string())
    res = await b.DifferentiateUnions({"tb": tb})
    assert isinstance(res, OriginalB)


@pytest.mark.asyncio
async def test_return_failing_assert():
    with pytest.raises(errors.BamlValidationError):
        msg = await b.ReturnFailingAssert(1)


@pytest.mark.asyncio
async def test_parameter_failing_assert():
    with pytest.raises(errors.BamlInvalidArgumentError):
        msg = await b.ReturnFailingAssert(100)
        assert msg == 103


@pytest.mark.asyncio
async def test_failing_assert_can_stream():
    stream = b.stream.StreamFailingAssertion("Yoshimi battles the pink robots", 300)
    async for msg in stream:
        print(msg.story_a)
        print(msg.story_b)
    with pytest.raises(errors.BamlValidationError):
        final = await stream.get_final_response()
        assert "Yoshimi" in final.story_a


@pytest.mark.asyncio
async def test_simple_recursive_type():
    res = await b.BuildLinkedList([1, 2, 3, 4, 5])
    assert res == LinkedList(
        len=5,
        head=Node(
            data=1,
            next=Node(
                data=2,
                next=Node(data=3, next=Node(data=4, next=Node(data=5, next=None))),
            ),
        ),
    )


@pytest.mark.asyncio
async def test_mutually_recursive_type():
    res = await b.BuildTree(
        BinaryNode(
            data=5,
            left=BinaryNode(
                data=3,
                left=BinaryNode(
                    data=1, left=BinaryNode(data=2, left=None, right=None), right=None
                ),
                right=BinaryNode(data=4, left=None, right=None),
            ),
            right=BinaryNode(
                data=7,
                left=BinaryNode(data=6, left=None, right=None),
                right=BinaryNode(data=8, left=None, right=None),
            ),
        )
    )
    assert res == Tree(
        data=5,
        children=Forest(
            trees=[
                Tree(
                    data=3,
                    children=Forest(
                        trees=[
                            Tree(
                                data=1,
                                children=Forest(
                                    trees=[Tree(data=2, children=Forest(trees=[]))]
                                ),
                            ),
                            Tree(data=4, children=Forest(trees=[])),
                        ]
                    ),
                ),
                Tree(
                    data=7,
                    children=Forest(
                        trees=[
                            Tree(data=6, children=Forest(trees=[])),
                            Tree(data=8, children=Forest(trees=[])),
                        ]
                    ),
                ),
            ]
        ),
    )


@pytest.mark.asyncio
async def test_block_constraints():
    ret = await b.MakeBlockConstraint()
    assert ret.checks["cross_field"].status == "failed"


@pytest.mark.asyncio
async def test_nested_block_constraints():
    ret = await b.MakeNestedBlockConstraint()
    print(ret)
    assert ret.nbc.checks["cross_field"].status == "succeeded"


@pytest.mark.asyncio
async def test_block_constraint_arguments():
    with pytest.raises(errors.BamlInvalidArgumentError) as e:
        block_constraint = BlockConstraintForParam(bcfp=1, bcfp2="too long!")
        await b.UseBlockConstraint(block_constraint)
    assert "Failed assert: hi" in str(e)

    with pytest.raises(errors.BamlInvalidArgumentError) as e:
        nested_block_constraint = NestedBlockConstraintForParam(nbcfp=block_constraint)
        await b.UseNestedBlockConstraint(nested_block_constraint)
    assert "Failed assert: hi" in str(e)

@pytest.mark.asyncio
async def test_null_literal_class_hello():
    stream = b.stream.NullLiteralClassHello(s="unused")
    async for msg in stream:
        msg.a is None


@pytest.mark.asyncio
async def test_semantic_streaming():
    stream = b.stream.MakeSemanticContainer()

    # We will use these to store streaming fields and check them
    # for stability.
    reference_string: Optional[str] = None
    reference_int: Optional[int] = None

    async for msg in stream:
        assert "string_with_twenty_words" in dict(msg)
        assert "sixteen_digit_number" in dict(msg)

        # Checks for stability of numeric and @stream.done fields.
        if msg.sixteen_digit_number is not None:
            if reference_int is None:
                # Set the reference if it hasn't been set yet.
                reference_int = msg.sixteen_digit_number
            else:
                # If the reference has been set, check that the
                # current value matches it.
                assert reference_int == msg.sixteen_digit_number
        if msg.string_with_twenty_words is not None:
            if reference_string is None:
                # Set the reference if it hasn't been set yet.
                reference_string = msg.string_with_twenty_words
            else:
                # If the reference has been set, check that the
                # current value matches it.
                assert reference_string == msg.string_with_twenty_words

        # Checks for @stream.with_state.
        if msg.class_needed is not None:
            if msg.class_needed.s_20_words.value is not None:
                if len(msg.class_needed.s_20_words.value.split(" ")) < 3 and msg.final_string is None:
                    print(msg)
                    assert msg.class_needed.s_20_words.state == "Incomplete"
        if msg.final_string is not None:
            assert msg.class_needed.s_20_words.state == "Complete"

        # Checks for @stream.not_null.
        for sub in msg.three_small_things:
            assert sub.i_16_digits is not None

    final = await stream.get_final_response()
    print(final)
