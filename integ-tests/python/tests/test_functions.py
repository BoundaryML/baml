import time
from typing import List
import pytest
from assertpy import assert_that
from dotenv import load_dotenv
from .base64_test_data import image_b64, audio_b64

load_dotenv()
import baml_py
from ..baml_client import b
from ..baml_client.sync_client import b as sync_b
from ..baml_client.globals import (
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
)
from ..baml_client import partial_types
from ..baml_client.types import (
    DynInputOutput,
    NamedArgsSingleEnumList,
    NamedArgsSingleClass,
    StringToClassEntry,
)
from ..baml_client.tracing import trace, set_tags, flush, on_log_event
from ..baml_client.type_builder import TypeBuilder
import datetime
import concurrent.futures
import asyncio
import random


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

    list = await b.FnOutputClassList(a)
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
async def test_claude():
    res = await b.PromptTestClaude(input="Mt Rainier is tall")
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_gemini():
    geminiRes = await b.TestGemini(input="Dr. Pepper")
    print(f"LLM output from Gemini: {geminiRes}")
    assert len(geminiRes) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_gemini_streaming():
    geminiRes = await b.stream.TestGemini(input="Dr. Pepper").get_final_response()
    print(f"LLM output from Gemini: {geminiRes}")

    assert len(geminiRes) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_aws():
    res = await b.TestAws(input="Mt Rainier is tall")
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
        assert msg.startswith(
            prev_msg
        ), "Expected messages to be continuous, but prev was %r and next was %r" % (
            prev_msg,
            msg,
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
        assert msg.startswith(
            prev_msg
        ), "Expected messages to be continuous, but prev was %r and next was %r" % (
            prev_msg,
            msg,
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
        assert msg.startswith(
            prev_msg
        ), "Expected messages to be continuous, but prev was %r and next was %r" % (
            prev_msg,
            msg,
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
        assert msg.startswith(
            prev_msg
        ), "Expected messages to be continuous, but prev was %r and next was %r" % (
            prev_msg,
            msg,
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
    assert output.hair_color == "black"


@pytest.mark.asyncio
async def test_dynamic_class_nested_output_no_stream():
    tb = TypeBuilder()
    nested_class = tb.add_class("Name")
    nested_class.add_property("first_name", tb.string())
    nested_class.add_property("last_name", tb.string().optional())
    nested_class.add_property("middle_name", tb.string().optional())

    other_nested_class = tb.add_class("Address")

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
    assert final.hair_color == "black"


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
    assert res[0].new_key == "hi1"
    assert res[0].testKey == "myTest"
    assert res[0].blah["nestedKey1"] == "nestedVal"
    assert res[1].new_key == "hi"
    assert res[1].testKey == "myTest"
    assert res[1].blah["nestedKey1"] == "nestedVal"


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
    assert res[0].new_key == "hi"
    assert res[0].testKey == "myTest"
    assert res[0].blah["nestedKey1"] == "nestedVal"
    assert res[1].new_key == "hi"
    assert res[1].testKey == "myTest"
    assert res[1].blah["nestedKey1"] == "nestedVal"


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
    assert res.hair_color == "black"
    assert res.attributes["eye_color"] == "blue"
    assert res.attributes["facial_hair"] == "beard"


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
    tb.DynamicOutput.add_property("height", tb.union(class1.type(), class2.type()))
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
    assert res.hair_color == "black"
    assert res.attributes["eye_color"] == "blue"
    assert res.attributes["facial_hair"] == "beard"
    assert res.height["feet"] == 6

    res = await b.MyFunc(
        input="My name is Harrison. My hair is black and I'm 1.8 meters tall. I have blue eyes and a beard. I am 30 years old.",
        baml_options={"tb": tb},
    )

    print("final ", res)
    print("final ", res.model_dump())
    print("final ", res.model_dump_json())
    assert res.hair_color == "black"
    assert res.attributes["eye_color"] == "blue"
    assert res.attributes["facial_hair"] == "beard"
    assert res.height["meters"] == 1.8


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
async def test_dynamic_clients():
    cb = baml_py.ClientRegistry()
    cb.add_llm_client("MyClient", "openai", {"model": "gpt-3.5-turbo"})
    cb.set_primary("MyClient")

    final = await b.TestOllama(
        input="My name is Harrison. My hair is black and I'm 6 feet tall.",
        baml_options={"client_registry": cb},
    )
    print("final ", final)


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
    # res = await b.TestAws("lightning in a rock")
    # print("unstreamed", res)

    ## streamed
    stream = b.stream.TestAws("lightning in a rock")

    async for msg in stream:
        if msg:
            print("streamed ", repr(msg[-100:]))

    res = await stream.get_final_response()
    print("streamed final", res)
    assert len(res) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_serialization_exception():
    with pytest.raises(Exception) as excinfo:
        await b.DummyOutputFunction("dummy input")

    print("Exception message: ", excinfo)
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
