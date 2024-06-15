import pytest
from dotenv import load_dotenv
from base64_test_data import image_b64, audio_b64


load_dotenv()
import baml_py
from baml_client import b
from baml_client.types import NamedArgsSingleEnumList, NamedArgsSingleClass
from baml_client.tracing import trace, set_tags, flush, on_log_event
from baml_client.type_builder import TypeBuilder
import datetime


@pytest.mark.asyncio
async def test_should_work_for_all_inputs():
    res = await b.TestFnNamedArgsSingleBool(True)
    assert res == "true"

    res = await b.TestFnNamedArgsSingleStringList(["a", "b", "c"])
    assert "a" in res and "b" in res and "c" in res

    print("calling with class")
    res = await b.TestFnNamedArgsSingleClass(
        myArg=NamedArgsSingleClass(
            key="key",
            key_two=True,
            key_three=52,
        )
    )
    print("got response", res)
    assert "52" in res

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

    res = await b.TestFnNamedArgsSingleEnumList([NamedArgsSingleEnumList.TWO])
    assert "TWO" in res

    res = await b.TestFnNamedArgsSingleFloat(3.12)
    assert "3.12" in res

    res = await b.TestFnNamedArgsSingleInt(3566)
    assert "3566" in res


class MyCustomClass(NamedArgsSingleClass):
    date: datetime.datetime


@pytest.mark.asyncio
async def accepts_subclass_of_baml_type():
    print("calling with class")
    res = await b.TestFnNamedArgsSingleClass(
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
    assert "green" in res.lower()


@pytest.mark.asyncio
async def test_should_work_with_image_base64():
    res = await b.TestImageInput(img=baml_py.Image.from_base64("image/png", image_b64))
    assert "green" in res.lower()


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

    geminiRes = await b.TestGemini(input="Dr. Pepper")
    print(f"LLM output from Gemini: {geminiRes}")

    assert len(geminiRes) > 0, "Expected non-empty result but got empty."


@pytest.mark.asyncio
async def test_streaming():
    stream = b.stream.PromptTestOpenAI(input="Programming languages are fun to create")
    msgs = []
    async for msg in stream:
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
    assert msgs[-1] == final, "Expected last stream message to match final response."


@pytest.mark.asyncio
async def test_streaming_uniterated():
    final = await b.stream.PromptTestOpenAI(
        input="The color blue makes me sad"
    ).get_final_response()
    assert len(final) > 0, "Expected non-empty final but got empty."


@pytest.mark.asyncio
async def test_streaming_claude():
    stream = b.stream.PromptTestClaude(input="Mt Rainier is tall")
    msgs = []
    async for msg in stream:
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
async def test_streaming_gemini():
    stream = b.stream.TestGemini(input="Dr.Pepper")
    msgs = []
    async for msg in stream:
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
async def test_tracing_async():
    # sync_dummy_func("second-dummycall-arg")
    res = await parent_async("first-arg-value")
    # sync_dummy_func("second-dummycall-arg")
    res2 = await parent_async2("second-arg-value")
    # sync_dummy_func("second-dummycall-arg")


def test_tracing_sync():
    # res = parent_sync("first-arg-value")
    res2 = sync_dummy_func("second-dummycall-arg")


def test_tracing_thread_pool():
    trace_thread_pool()


@pytest.mark.asyncio
async def test_tracing_thread_pool_async():
    await trace_thread_pool_async()


@pytest.mark.asyncio
async def test_tracing_async_gather():
    await trace_async_gather()


import concurrent.futures


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
            res = await future.result()


@trace
async def trace_async_gather():
    await asyncio.gather(*[async_dummy_func("second-dummycall-arg") for _ in range(10)])


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


import asyncio
import random


@trace
async def async_dummy_func(myArgggg: str):
    await asyncio.sleep(0.5 + random.random())
    return "asyncDummyFuncOutput"


@trace
def sync_dummy_func(dummyFuncArg: str):
    return "pythonDummyFuncOutput"


@pytest.fixture(scope="session", autouse=True)
def cleanup(request):
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
    msgs = []
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

    stream = b.stream.MyFunc(
        input="My name is Harrison. My hair is black and I'm 6 feet tall.",
        baml_options={"tb": tb},
    )
    msgs = []
    async for msg in stream:
        print("streamed ", msg)
        print("streamed ", msg.model_dump())
        msgs.append(msg)
    final = await stream.get_final_response()

    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    print("final ", final)
    print("final ", final.model_dump())
    print("final ", final.model_dump_json())
    assert final.hair_color == "black"


@pytest.mark.asyncio
async def test_nested_class_streaming():
    stream = b.stream.FnOutputClassNested(
        input="My name is Harrison. My hair is black and I'm 6 feet tall."
    )
    msgs = []
    async for msg in stream:
        print("streamed ", msg.model_dump(mode="json"))
        msgs.append(msg)
    final = await stream.get_final_response()

    assert len(msgs) > 0, "Expected at least one streamed response but got none."
    print("final ", final.model_dump(mode="json"))


@pytest.mark.asyncio
async def test_event_log_hook():
    def event_log_hook(event):
        print("Event log hook1: ")
        print("Event log event ", event)

    on_log_event(event_log_hook)
    res = await b.TestFnNamedArgsSingleBool(True)
    assert res == "true"
