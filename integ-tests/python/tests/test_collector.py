from baml_py.errors import BamlInvalidArgumentError, BamlError, BamlClientError
from baml_py.baml_py import LLMCall, LLMStreamCall
import pytest
from openai.types.chat import ChatCompletion

from ..baml_client import b
from ..baml_client.async_client import BamlCallOptions
from ..baml_client.sync_client import b as b_sync
from baml_py import ClientRegistry, Collector
import gc
import sys
import asyncio
from contextlib import asynccontextmanager


def function_call_count():
    return Collector.__function_call_count()  # type: ignore


@pytest.fixture(autouse=True)
def ensure_collector_is_empty():
    assert function_call_count() == 0
    yield
    gc.collect()
    assert function_call_count() == 0


@pytest.mark.asyncio
async def test_collector_async_no_stream_success():
    # garbage collected!
    assert function_call_count() == 0

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
    response_body = response.body.json()
    assert response.status == 200
    assert response_body is not None
    assert isinstance(response_body, dict)
    assert "choices" in response_body
    assert len(response_body["choices"]) > 0
    assert "message" in response_body["choices"][0]
    assert "content" in response_body["choices"][0]["message"]
    completion = ChatCompletion(**response_body)
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
    assert function_call_count() > 0


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
    assert function_call_count() > 0


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
    assert log.log_type == "stream"

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "stream"

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
    assert function_call_count() > 0


@pytest.mark.asyncio
async def test_collector_async_stream_success_gemini():
    collector = Collector(name="my-collector")
    function_logs = collector.logs
    assert len(function_logs) == 0

    stream = b.stream.TestGemini("hi there", baml_options={"collector": collector})

    async for chunk in stream:
        print(f"### chunk: {chunk}")

    res = await stream.get_final_response()
    print(f"### res: {res}")
    function_logs = collector.logs

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestGemini"
    assert log.log_type == "stream"

    function_logs = collector.logs
    assert len(function_logs) == 1

    log = collector.last
    assert log is not None
    assert log.function_name == "TestGemini"
    assert log.log_type == "stream"

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

    assert call.provider == "google-ai"
    assert call.client_name == "Gemini"
    assert call.selected

    # Verify request/response
    request = call.http_request
    assert request is not None
    print(f"### request.body: {request.body} \n {type(request.body)}", file=sys.stderr)
    body = request.body.json()

    print(f"{call.usage}")

    assert isinstance(body, dict)
    assert "contents" in body

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
    assert function_call_count() > 0


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
    usage_call1 = logs[0].usage
    usage_call2 = logs[1].usage
    assert usage_call1 is not None
    assert usage_call2 is not None
    assert usage_call1.input_tokens is not None
    assert usage_call2.input_tokens is not None
    assert usage_call1.output_tokens is not None
    assert usage_call2.output_tokens is not None

    # # Verify that total collector usage equals the sum of the two logs
    total_input = usage_call1.input_tokens + usage_call2.input_tokens
    total_output = usage_call1.output_tokens + usage_call2.output_tokens
    assert collector.usage.input_tokens == total_input
    assert collector.usage.output_tokens == total_output


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


@pytest.mark.asyncio
async def test_collector_aws_bedrock():
    collector = Collector(name="my-collector")
    await b.TestAws("hi there", baml_options={"collector": collector})
    logs = collector.logs
    assert len(logs) == 1
    assert logs[0].function_name == "TestAws"

    # Verify the HTTP request body for AWS Bedrock
    log = logs[0]
    calls = log.calls
    print("------------------------- calls", calls)
    assert len(calls) == 1

    call = calls[0]
    assert call.provider == "aws-bedrock"

    # Verify request
    request = call.http_request
    assert request is not None
    body = request.body.json()
    assert isinstance(body, dict)
    assert "inferenceConfig" in body


@pytest.mark.asyncio
async def test_collector_vertex():
    collector = Collector(name="my-collector")
    await b.TestVertex("donkey kong", baml_options={"collector": collector})
    logs = collector.logs
    assert len(logs) == 1
    assert logs[0].function_name == "TestVertex"
    assert logs[0].log_type == "call"

    call = logs[0].calls[0]
    assert call.provider == "vertex-ai"
    assert call.client_name == "Vertex"
    assert call.selected

    # Verify request
    request = call.http_request
    assert request is not None
    body = request.body.json()
    assert isinstance(body, dict)

    # Verify response
    response = call.http_response
    assert response is not None
    response_body = response.body.json()
    assert isinstance(response_body, dict)
    assert "candidates" in response_body


@pytest.mark.asyncio
async def test_collector_gemini():
    collector = Collector(name="my-collector")
    geminiRes = await b.TestGemini(
        input="Dr. Pepper", baml_options={"collector": collector}
    )
    print(f"LLM output from Gemini: {geminiRes}")
    assert len(geminiRes) > 0, "Expected non-empty result but got empty."

    logs = collector.logs
    assert len(logs) == 1
    assert logs[0].function_name == "TestGemini"
    assert logs[0].log_type == "call"

    call = logs[0].calls[0]
    assert call.provider == "google-ai"
    assert call.client_name == "Gemini"
    assert call.selected

    # Verify request
    request = call.http_request
    assert request is not None
    body = request.body.json()
    assert isinstance(body, dict)

    # Verify response
    response = call.http_response
    assert response is not None
    response_body = response.body.json()
    assert isinstance(response_body, dict)


@pytest.mark.asyncio
async def test_collector_claude():
    collector = Collector(name="my-collector")
    res = await b.PromptTestClaude(
        input="Mt Rainier is tall", baml_options={"collector": collector}
    )
    assert len(res) > 0, "Expected non-empty result but got empty."

    logs = collector.logs
    assert len(logs) == 1
    assert logs[0].function_name == "PromptTestClaude"
    assert logs[0].log_type == "call"

    call = logs[0].calls[0]
    assert call.provider == "anthropic"
    assert call.selected

    # Verify request
    request = call.http_request
    assert request is not None
    body = request.body.json()
    assert isinstance(body, dict)

    # Verify response
    response = call.http_response
    assert response is not None
    response_body = response.body.json()
    assert isinstance(response_body, dict)
    assert "content" in response_body
    assert len(response_body["content"]) > 0


@pytest.mark.asyncio
async def test_collector_groq():
    collector = Collector(name="my-collector")
    res = await b.TestGroq("hi there", baml_options={"collector": collector})
    assert len(res) > 0, "Expected non-empty result but got empty."
    assert collector.usage.input_tokens is not None
    assert collector.usage.output_tokens is not None
    assert collector.usage.input_tokens > 0
    assert collector.usage.output_tokens > 0


@pytest.mark.asyncio
async def test_collector_multiple_async_nested():
    from baml_client.tracing import trace

    collector = Collector(name="my-collector")

    @trace
    async def more_nested():
        return "hi"

    @trace
    async def gather_batch_2():
        await more_nested()
        await more_nested()
        return await asyncio.gather(
            b.TestOpenAIGPT4oMini2("hi there", baml_options={"collector": collector}),
            # context depth 2 after enter()
            b.TestOpenAIGPT4oMini3("hi there", baml_options={"collector": collector}),
            # more_nested()
        )

    async def gather_batch_1():
        # all these have context depth 1 initially
        return await asyncio.gather(
            b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector}),
            gather_batch_2(),
        )

    # batch_1_results = await asyncio.gather(gather_batch_1())
    await gather_batch_1()

    # assert collector.usage.input_tokens is not None
    # assert collector.usage.output_tokens is not None
    # assert collector.usage.input_tokens > 0
    # assert collector.usage.output_tokens > 0


@pytest.mark.asyncio
async def test_collector_multiple_async_nested_stream():
    from baml_client.tracing import trace

    collector = Collector(name="my-collector")

    @trace
    async def more_nested():
        return "hi"

    async def stream1():
        stream = b.stream.TestOpenAIGPT4oMini2(
            "hi there", baml_options={"collector": collector}
        )
        async for chunk in stream:
            print(f"stream1: {chunk}")
        return "done"

    @trace
    async def gather_batch_2():
        await more_nested()
        return await asyncio.gather(
            stream1(),
            # context depth 2 after enter()
            # b.stream.TestOpenAIGPT4oMini3("hi there", baml_options={"collector": collector}),
            more_nested(),
        )

    async def gather_batch_1():
        # all these have context depth 1 initially
        return await asyncio.gather(
            b.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector}),
            gather_batch_2(),
        )

    # batch_1_results = await asyncio.gather(gather_batch_1())
    await gather_batch_1()

    # assert collector.usage.input_tokens is not None
    # assert collector.usage.output_tokens is not None
    # assert collector.usage.input_tokens > 0
    # assert collector.usage.output_tokens > 0


@pytest.mark.asyncio
async def test_collector_multiple_sync_nested():
    from baml_client.tracing import trace

    collector = Collector(name="my-collector")

    @trace
    def more_nested():
        return "hi"

    @trace
    def gather_batch_2():
        more_nested()
        # return await asyncio.gather(
        b_sync.TestOpenAIGPT4oMini("hi there", baml_options={"collector": collector})
        # )

    # batch_1_results = await asyncio.gather(gather_batch_1())
    gather_batch_2()

    assert collector.usage.input_tokens is not None
    assert collector.usage.output_tokens is not None
    assert collector.usage.input_tokens > 0
    assert collector.usage.output_tokens > 0


@pytest.mark.asyncio
async def test_collector_context_manager_pattern():
    """Test using collector with context manager pattern similar to production usage"""

    # Mock usage tracking (similar to your _ModelUsage class)
    class MockUsageTracker:
        def __init__(self):
            self.usage_by_provider: dict[str, dict[str, int]] = {}

        def record_usage(self, provider: str, input_tokens: int, output_tokens: int):
            if provider not in self.usage_by_provider:
                self.usage_by_provider[provider] = {"input": 0, "output": 0}
            self.usage_by_provider[provider]["input"] += input_tokens
            self.usage_by_provider[provider]["output"] += output_tokens

    def record_baml_usage(usage_tracker: MockUsageTracker, baml_collector: Collector):
        """Record usage from collector logs (similar to your _record_baml_usage)"""
        for log in baml_collector.logs:
            for call in log.calls:
                usage_tracker.record_usage(
                    provider=call.provider,
                    input_tokens=call.usage.input_tokens or 0,
                    output_tokens=call.usage.output_tokens or 0,
                )

    @asynccontextmanager
    async def baml_instrumentation(name: str):
        """Context manager for BAML instrumentation (similar to your pattern)"""
        baml_collector = Collector(name=name)
        usage_tracker = MockUsageTracker()
        try:
            yield baml_collector, usage_tracker
        finally:
            record_baml_usage(usage_tracker, baml_collector)

    async def process_text(
        text: str,
        baml_options: BamlCallOptions,
    ) -> str:
        """Wrapper function that calls BAML function (similar to your sanitize_text)"""
        return await b.TestOpenAIGPT4oMini(text, baml_options=baml_options)

    async def process_text_batch_item(
        text: str,
        baml_options: BamlCallOptions,
    ) -> str:
        """Another wrapper (similar to your _sanitize_literal_text_row)"""
        return await process_text(text=text, baml_options=baml_options)

    async def process_text_batch(
        texts: list[str],
        baml_options: BamlCallOptions,
    ) -> list[str]:
        """Batch processing with parallel execution (similar to your _sanitize_literal_text)"""
        process_tasks = [process_text_batch_item(text, baml_options) for text in texts]
        return await asyncio.gather(*process_tasks)

    # Test the pattern
    test_texts = ["Hello world", "How are you?", "Test message"]

    async with baml_instrumentation("test-context-manager") as (
        collector,
        usage_tracker,
    ):
        results = await process_text_batch(
            texts=test_texts, baml_options={"collector": collector}
        )

    # Verify results
    assert len(results) == 3
    assert all(isinstance(result, str) and len(result) > 0 for result in results)

    # Verify collector captured all calls
    logs = collector.logs
    assert len(logs) == 3

    # Verify all logs have the expected function name
    for log in logs:
        assert log.function_name == "TestOpenAIGPT4oMini"
        assert log.log_type == "call"

    # Verify usage was recorded correctly
    assert "openai" in usage_tracker.usage_by_provider
    openai_usage = usage_tracker.usage_by_provider["openai"]
    assert openai_usage["input"] > 0
    assert openai_usage["output"] > 0

    # Verify collector totals match usage tracker totals
    assert collector.usage.input_tokens == openai_usage["input"]
    assert collector.usage.output_tokens == openai_usage["output"]

    # Verify timing - all calls should have completed
    for log in logs:
        assert log.timing.duration_ms is not None and log.timing.duration_ms > 0


@pytest.mark.asyncio
async def test_collector_mixed_providers_context_manager():
    """Test context manager pattern with multiple providers"""

    class UsageTracker:
        def __init__(self):
            self.total_input = 0
            self.total_output = 0
            self.providers = set()

        def add_from_collector(self, collector: Collector):
            for log in collector.logs:
                for call in log.calls:
                    self.providers.add(call.provider)
                    self.total_input += call.usage.input_tokens or 0
                    self.total_output += call.usage.output_tokens or 0

    @asynccontextmanager
    async def multi_provider_context():
        collector = Collector(name="multi-provider-test")
        tracker = UsageTracker()
        try:
            yield collector, tracker
        finally:
            tracker.add_from_collector(collector)

    async def call_different_providers(collector: Collector) -> list[str]:
        """Call different BAML functions with different providers"""
        tasks = [
            b.TestOpenAIGPT4oMini("test openai", baml_options={"collector": collector}),
            b.TestGroq("test groq", baml_options={"collector": collector}),
        ]
        return await asyncio.gather(*tasks)

    async with multi_provider_context() as (collector, tracker):
        results = await call_different_providers(collector)

    # Verify results
    assert len(results) == 2
    assert all(isinstance(result, str) and len(result) > 0 for result in results)

    # Verify multiple providers were used
    assert len(tracker.providers) >= 2
    assert "openai" in tracker.providers
    assert "openai-generic" in tracker.providers

    # Verify usage was tracked
    assert tracker.total_input > 0
    assert tracker.total_output > 0

    # Verify collector logs
    assert len(collector.logs) == 2


@pytest.mark.asyncio
async def test_collector_openai_stream_chunk_verification():
    """Test streaming collector for OpenAI with detailed chunk-by-chunk verification"""
    collector = Collector(name="openai-stream-chunks")

    # Track chunks as they arrive
    chunks_received = []
    stream = b.stream.TestOpenAIGPT4oMini(
        "Count from 1 to 5", baml_options={"collector": collector}
    )

    async for chunk in stream:
        chunks_received.append(chunk)
        print(f"Received chunk: {chunk}")

    # Get final response
    final_response = await stream.get_final_response()

    # Verify we received multiple chunks
    assert len(chunks_received) > 1, "Should receive multiple chunks in a stream"

    # Verify final response is complete
    assert len(final_response) > 0

    # Verify collector captured the stream
    logs = collector.logs
    assert len(logs) == 1

    log = logs[0]
    assert log.function_name == "TestOpenAIGPT4oMini"
    assert log.log_type == "stream"

    # Verify timing for streaming
    assert log.timing.start_time_utc_ms > 0
    assert log.timing.duration_ms is not None and log.timing.duration_ms > 0

    # Verify usage is captured for streaming
    assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
    assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

    # Verify call details
    call = log.calls[0]
    assert not isinstance(call, LLMCall)
    assert isinstance(call, LLMStreamCall)
    assert call.provider == "openai"
    assert call.client_name == "GPT4oMini"
    sse_chunks = call.sse_responses()
    assert sse_chunks is not None
    assert len(sse_chunks) >= len(
        chunks_received
    ), f"Expected {len(chunks_received)} chunks, got {sse_chunks}"
    for chunk in sse_chunks:
        print(f"Chunk: {chunk.json()}")

    # For streaming, http response should be None (as noted in existing test)
    assert call.http_response is None

    # But request should exist
    assert call.http_request is not None
    request_body = call.http_request.body.json()
    assert request_body.get("stream") is True  # Verify streaming was requested


@pytest.mark.asyncio
async def test_collector_openai_multiple_concurrent_streams():
    """Test streaming collector with multiple concurrent OpenAI streams"""
    collector = Collector(name="openai-concurrent-streams")

    async def stream_and_collect(prompt: str) -> tuple[list[str], str]:
        """Helper to run a stream and collect chunks"""
        chunks = []
        stream = b.stream.TestOpenAIGPT4oMini(
            prompt, baml_options={"collector": collector}
        )

        async for chunk in stream:
            chunks.append(chunk)

        final = await stream.get_final_response()
        return chunks, final

    # Run multiple streams concurrently
    results = await asyncio.gather(
        stream_and_collect("Say hello"),
        stream_and_collect("Say goodbye"),
        stream_and_collect("Count to 3"),
    )

    # Verify we got results from all streams
    assert len(results) == 3
    for chunks, final in results:
        assert len(chunks) > 0
        assert len(final) > 0

    # Verify collector captured all streams
    logs = collector.logs
    assert len(logs) == 3

    # Verify each log is properly formed
    for i, log in enumerate(logs):
        assert log.function_name == "TestOpenAIGPT4oMini"
        assert log.log_type == "stream"
        assert log.timing.duration_ms is not None and log.timing.duration_ms > 0
        assert log.usage.input_tokens is not None and log.usage.input_tokens > 0
        assert log.usage.output_tokens is not None and log.usage.output_tokens > 0

        # Verify streaming request
        call = log.calls[0]
        assert call.provider == "openai"
        assert call.http_request
        request_body = call.http_request.body.json()
        assert request_body.get("stream") is True

    # Verify total usage is sum of all streams
    total_input = sum(log.usage.input_tokens or 0 for log in logs)
    total_output = sum(log.usage.output_tokens or 0 for log in logs)
    assert collector.usage.input_tokens == total_input
    assert collector.usage.output_tokens == total_output


@pytest.mark.asyncio
async def test_collector_openai_stream_usage_accumulation():
    """Test that streaming collector properly accumulates usage across multiple calls"""
    collector = Collector(name="openai-stream-usage")

    # First streaming call
    stream1 = b.stream.TestOpenAIGPT4oMini(
        "First stream", baml_options={"collector": collector}
    )
    async for _ in stream1:
        pass
    await stream1.get_final_response()

    # Capture usage after first stream
    first_usage = collector.usage
    first_input = first_usage.input_tokens
    first_output = first_usage.output_tokens
    assert first_input is not None and first_input > 0
    assert first_output is not None and first_output > 0

    # Second streaming call
    stream2 = b.stream.TestOpenAIGPT4oMini(
        "Second stream with more content", baml_options={"collector": collector}
    )
    async for _ in stream2:
        pass
    await stream2.get_final_response()

    # Verify usage accumulated
    assert collector.usage.input_tokens is not None
    assert collector.usage.output_tokens is not None
    assert collector.usage.input_tokens > first_input
    assert collector.usage.output_tokens > first_output
    assert collector.usage.input_tokens > 0
    assert collector.usage.output_tokens > 0

    # Non-streaming call
    await b.TestOpenAIGPT4oMini(
        "Non-streaming call", baml_options={"collector": collector}
    )

    # Verify we have 3 logs total
    logs = collector.logs
    assert len(logs) == 3

    # Verify total usage matches sum of individual calls
    total_input = sum(log.usage.input_tokens or 0 for log in logs)
    total_output = sum(log.usage.output_tokens or 0 for log in logs)
    assert collector.usage.input_tokens == total_input
    assert collector.usage.output_tokens == total_output

    # Verify first two are streaming, last is not
    for i in range(2):
        request_body = logs[i].calls[0].http_request.body.json()
        assert request_body.get("stream") is True

    # Last call should not be streaming
    last_request_body = logs[2].calls[0].http_request.body.json()
    assert last_request_body.get("stream") is not True
