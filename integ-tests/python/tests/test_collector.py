from baml_py.errors import BamlInvalidArgumentError, BamlError, BamlClientError
import pytest
import dotenv
from openai.types.chat import ChatCompletion

from ..baml_client import b
from ..baml_client.sync_client import b as b_sync
from baml_py import ClientRegistry, Collector
import gc
import sys
import asyncio

dotenv.load_dotenv()


def function_span_count():
    return Collector.__function_span_count()  # type: ignore


@pytest.fixture(autouse=True)
def ensure_collector_is_empty():
    assert function_span_count() == 0
    yield
    gc.collect()
    assert function_span_count() == 0


@pytest.mark.asyncio
async def test_collector_async_no_stream_success():
    # garbage collected!
    assert function_span_count() == 0

    collector = Collector(name="my-collector")
    function_logs = collector.logs
    # print("### function_logs", function_logs, file=sys.stderr)
    assert len(function_logs) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})

    function_logs = collector.logs
    # print("### function_logs2", function_logs, file=sys.stderr)
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "call"

    # Verify timing fields
    assert log.timing.start_time_utc_ms > 0
    assert log.timing.duration_ms is not None and log.timing.duration_ms > 0

    # TODO: add this api
    # assert log.timing.time_to_first_parsed_ms is not None
    # assert log.timing.time_to_first_parsed_ms > 0

    # Verify usage fields
    assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert len(calls) == 1

    call = calls[0]

    assert call.provider == "openai"
    assert call.client_name == "GPT4oMini"
    assert call.selected

    # Verify request/response
    request = call.http_request
    assert request is not None
    print(f"### request.body: {request.body} \n {type(request.body)}", file=sys.stderr)
    body = request.body.json()
    assert isinstance(body, dict)
    assert "messages" in body
    assert "content" in body["messages"][0]
    assert body["messages"][0]["content"] is not None
    assert body["model"] == "gpt-4o-mini"

    # Verify http response
    response = call.http_response
    assert response is not None
    assert response.status == 200
    assert response.body is not None
    assert isinstance(response.body, dict)
    completion = ChatCompletion(**response.body)
    assert "choices" in response.body
    assert len(response.body["choices"]) > 0
    assert "message" in response.body["choices"][0]
    assert "content" in response.body["choices"][0]["message"]
    assert completion.choices[0].message.content is not None

    # Verify call timing
    call_timing = call.timing
    assert call_timing.start_time_utc_ms > 0
    assert call_timing.duration_ms is not None and call_timing.duration_ms > 0

    # Verify call usage
    call_usage = call.usage
    assert call_usage.input_tokens is not None and call_usage.input_tokens > 0
    assert call_usage.output_tokens is not None and call_usage.output_tokens > 0
    # it matches the log usage
    assert call_usage.input_tokens == log.usage.input_tokens
    assert call_usage.output_tokens == log.usage.output_tokens

    # Verify raw response exists
    assert log.raw_llm_response is not None

    assert collector.usage.input_tokens == log.usage.input_tokens
    assert collector.usage.output_tokens == log.usage.output_tokens

    # Verify metadata
    assert isinstance(log.metadata, dict)

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert function_span_count() > 0


@pytest.mark.asyncio
async def test_collector_async_no_stream_no_getting_logs():
    collector = Collector(name="my-collector")
    function_logs = collector.logs
    assert len(function_logs) == 0

    await b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})
    # async for chunk in stream:
    #     print(f"### chunk: {chunk}")

    # TODO: possible bug -- if no functionLog pyo3 objects are created, that function ref count is always 1
    # and it never goes away.
    # function_logs = collector.logs

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert function_span_count() > 0


@pytest.mark.asyncio
async def test_collector_async_stream_success():
    collector = Collector(name="my-collector")
    function_logs = collector.logs
    assert len(function_logs) == 0

    stream = b.stream.TestOpenAIGPT4oMini(
        "hi there", baml_options={"collector": collector}
    )

    async for chunk in stream:
        print(f"### chunk: {chunk}")

    res = await stream.get_final_response()
    print(f"### res: {res}")
    function_logs = collector.logs

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "call"

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "call"

    # Verify timing fields
    assert log.timing.start_time_utc_ms > 0
    assert log.timing.duration_ms is not None and log.timing.duration_ms > 0

    # Verify usage fields
    assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # Verify calls
    calls = log.calls
    assert len(calls) == 1

    call = calls[0]

    assert call.provider == "openai"
    assert call.client_name == "GPT4oMini"
    assert call.selected

    # Verify request/response
    request = call.http_request
    assert request is not None
    print(f"### request.body: {request.body} \n {type(request.body)}", file=sys.stderr)
    body = request.body.json()
    assert isinstance(body, dict)
    assert "messages" in body

    # Verify http response
    response = call.http_response
    assert response is None

    # Verify call timing
    call_timing = call.timing
    assert call_timing.start_time_utc_ms > 0
    assert call_timing.duration_ms is not None and call_timing.duration_ms > 0

    # Verify call usage
    call_usage = call.usage
    assert call_usage.input_tokens is not None and call_usage.input_tokens > 0
    assert call_usage.output_tokens is not None and call_usage.output_tokens > 0

    # Verify raw response exists
    assert log.raw_llm_response is not None

    gc.collect()
    print("----- gc.collect() -----", file=sys.stderr)
    # still not collected cause it's in use
    assert function_span_count() > 0


@pytest.mark.asyncio
async def test_collector_async_multiple_calls_usage():
    collector = Collector(name="my-collector")

    # First call
    await b.TestOpenAIGPT4oMini("First call", baml_options={"collector": collector})
    function_logs = collector.logs
    assert len(function_logs) == 1

    # Capture usage after first call
    first_call_usage = function_logs[0].usage
    assert collector.usage.input_tokens == first_call_usage.input_tokens
    assert collector.usage.output_tokens == first_call_usage.output_tokens

    # Second call
    await b.TestOpenAIGPT4oMini("Second call", baml_options={"collector": collector})
    function_logs = collector.logs
    assert len(function_logs) == 2

    # Capture usage after second call and verify it's the sum of both calls
    second_call_usage = function_logs[1].usage
    total_input = (first_call_usage.input_tokens or 0) + (
        second_call_usage.input_tokens or 0
    )
    total_output = (first_call_usage.output_tokens or 0) + (
        second_call_usage.output_tokens or 0
    )
    assert collector.usage.input_tokens == total_input
    assert collector.usage.output_tokens == total_output


@pytest.mark.asyncio
async def test_collector_multiple_collectors():
    coll1 = Collector(name="collector-1")
    coll2 = Collector(name="collector-2")

    # Pass in both collectors for the first call
    await b.TestOpenAIGPT4oMini(
        "First call", baml_options={"collector": [coll1, coll2]}
    )

    # Check usage/logs after the first call
    logs1 = coll1.logs
    logs2 = coll2.logs
    assert len(logs1) == 1
    assert len(logs2) == 1

    usage_first_call_coll1 = logs1[0].usage
    usage_first_call_coll2 = logs2[0].usage

    # Verify both collectors have the exact same usage for the first call
    assert usage_first_call_coll1.input_tokens == usage_first_call_coll2.input_tokens
    assert usage_first_call_coll1.output_tokens == usage_first_call_coll2.output_tokens

    # Also check that the collector-level usage matches the single call usage for each collector
    assert coll1.usage.input_tokens == usage_first_call_coll1.input_tokens
    assert coll1.usage.output_tokens == usage_first_call_coll1.output_tokens
    assert coll2.usage.input_tokens == usage_first_call_coll2.input_tokens
    assert coll2.usage.output_tokens == usage_first_call_coll2.output_tokens

    # Second call uses only coll1
    await b.TestOpenAIGPT4oMini("Second call", baml_options={"collector": coll1})

    # Re-check logs/usage
    logs1 = coll1.logs
    logs2 = coll2.logs
    assert len(logs1) == 2
    assert len(logs2) == 1

    # Verify coll1 usage is now the sum of both calls
    usage_second_call_coll1 = logs1[1].usage
    total_input = (usage_first_call_coll1.input_tokens or 0) + (
        usage_second_call_coll1.input_tokens or 0
    )
    total_output = (usage_first_call_coll1.output_tokens or 0) + (
        usage_second_call_coll1.output_tokens or 0
    )
    assert coll1.usage.input_tokens == total_input
    assert coll1.usage.output_tokens == total_output

    # Verify coll2 usage remains unchanged (it did not participate in the second call)
    assert coll2.usage.input_tokens == usage_first_call_coll2.input_tokens
    assert coll2.usage.output_tokens == usage_first_call_coll2.output_tokens


@pytest.mark.asyncio
async def test_collector_mixed_async_sync_calls():
    collector = Collector(name="mixed-collector")

    # First, an async call
    await b.TestOpenAIGPT4oMini("async call #1", baml_options={"collector": collector})
    logs = collector.logs
    assert len(logs) == 1
    usage_first_call = logs[0].usage

    # Verify collector usage matches the first call's usage
    assert collector.usage.input_tokens == usage_first_call.input_tokens
    assert collector.usage.output_tokens == usage_first_call.output_tokens

    # Next, a sync call
    b_sync.TestOpenAIGPT4oMini("sync call #2", baml_options={"collector": collector})
    logs = collector.logs
    assert len(logs) == 2

    # Verify the second call's usage
    usage_second_call = logs[1].usage
    assert logs[1].timing.start_time_utc_ms > logs[0].timing.start_time_utc_ms
    total_input = (usage_first_call.input_tokens or 0) + (
        usage_second_call.input_tokens or 0
    )
    total_output = (usage_first_call.output_tokens or 0) + (
        usage_second_call.output_tokens or 0
    )
    assert collector.usage.input_tokens == total_input
    assert collector.usage.output_tokens == total_output


@pytest.mark.asyncio
async def test_collector_parallel_async_calls():
    collector = Collector(name="parallel-collector")

    # Execute two calls in parallel
    await asyncio.gather(
        b.TestOpenAIGPT4oMini("call #1", baml_options={"collector": collector}),
        b.TestOpenAIGPT4oMini("call #2", baml_options={"collector": collector}),
    )
    print("------------------------- ended parallel calls")

    # Verify the collector has two function logs
    logs = collector.logs
    # assert len(logs) == 2

    # Ensure each call is recorded properly
    print("------------------------- logs iteration", logs)
    # TODO: try this loop in earlier test and see if it works as well.
    for log in logs:
        assert log.function_name == "TestOpenAIGPT4oMini"
        assert log.log_type == "call"

    # # Check usage for each call
    # usage_call1 = logs[0].usage
    # usage_call2 = logs[1].usage
    # assert usage_call1 is not None
    # assert usage_call2 is not None

    # # Verify that total collector usage equals the sum of the two logs
    # total_input = usage_call1.input_tokens + usage_call2.input_tokens
    # total_output = usage_call1.output_tokens + usage_call2.output_tokens
    # assert collector.usage.input_tokens == total_input
    # assert collector.usage.output_tokens == total_output


@pytest.mark.asyncio
async def test_collector_failures_arg_type():
    collector = Collector(name="my-collector")
    with pytest.raises(BamlInvalidArgumentError):
        value: str = 124  # type: ignore (We want to test the error)
        await b.TestOpenAIGPT4oMini(value, baml_options={"collector": collector})

    assert len(collector.logs) == 1
    last_log = collector.last
    print("------------------------- last_log", last_log)
    assert last_log is not None
    assert last_log.function_name == "TestOpenAIGPT4oMini"


@pytest.mark.asyncio
async def test_collector_failures_client_registry():
    collector = Collector(name="my-collector")
    client_registry = ClientRegistry()
    client_registry.set_primary("DoesNotExist")
    with pytest.raises(BamlError):
        await b.TestOpenAIGPT4oMini(
            "hi there",
            baml_options={"collector": collector, "client_registry": client_registry},
        )
    assert len(collector.logs) == 1
    last_log = collector.last
    assert last_log is not None
    assert last_log.function_name == "TestOpenAIGPT4oMini"


@pytest.mark.asyncio
async def test_collector_failures_arg_type_streaming():
    collector = Collector(name="my-collector")
    with pytest.raises(BamlInvalidArgumentError):
        value: str = 124  # type: ignore (We want to test the error)
        async for _ in b.stream.TestOpenAIGPT4oMini(
            value, baml_options={"collector": collector}
        ):
            pass

    # Fails before the stream is even started
    # We don't have a state for streams that were "registered" but not started
    assert len(collector.logs) == 0


@pytest.mark.asyncio
async def test_collector_failures_client_registry_streaming():
    collector = Collector(name="my-collector")
    client_registry = ClientRegistry()
    client_registry.add_llm_client(
        "TestClient",
        "openai",
        {"model": "gpt-4o-mini", "base_url": "https://does-not-exist.com"},
    )
    client_registry.set_primary("TestClient")
    with pytest.raises(BamlClientError):
        try:
            stream = b.stream.TestOpenAIGPT4oMini(
                "hi there",
                baml_options={
                    "collector": collector,
                    "client_registry": client_registry,
                },
            )
            # TODO: baml doesnt yet throw if theres a connection error during the stream..
            async for _ in stream:
                pass
            # So we try to call get final response to make sure it fails
            await stream.get_final_response()
        except Exception as e:
            print(f"Error occurred: {e}")
            raise
    assert len(collector.logs) == 1
    last_log = collector.last
    assert last_log is not None
    assert last_log.function_name == "TestOpenAIGPT4oMini"
