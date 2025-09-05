import pytest
import json
from typing import List, Tuple, Optional
from baml_py import baml_py
from ..baml_client import b  # async client
from ..baml_client.tracing import flush


def get_on_tick() -> Tuple[callable, List[Tuple[str, Optional[str]]], List[int]]:
    """Helper function to create on_tick callback with state tracking

    Returns:
        - on_tick callback function
        - tick_events list [(reason, thinking_content), ...]
        - tick_counts list [count]
    """
    tick_events = []
    tick_counts = [0]
    last_thinking = [""]

    def on_tick(reason: str, log: baml_py.FunctionLog):
        tick_counts[0] += 1

        # Get the last call from the function log
        if log and log.calls:
            last_call = log.calls[-1]

            # Check if it's a streaming call and extract thinking
            if hasattr(last_call, "sse_responses"):
                sse_responses = last_call.sse_responses()
                if sse_responses:
                    for response in sse_responses:
                        try:
                            text = response.text
                            # Parse JSON and extract thinking if present
                            data = json.loads(text)
                            if "delta" in data and "thinking" in data["delta"]:
                                thinking = data["delta"]["thinking"]
                                last_thinking[0] += thinking
                        except (json.JSONDecodeError, AttributeError):
                            pass

        tick_events.append((reason, last_thinking[0]))

    return on_tick, tick_events, tick_counts


@pytest.mark.asyncio
async def test_ontick_request():
    """Test on_tick with non-streaming function call"""
    flush()  # Clear any existing state

    on_tick, tick_events, tick_counts = get_on_tick()

    # Call function with on_tick (using TestAnthropicShorthand first to test mechanism)
    result = await b.TestAnthropicShorthand(
        input="A robot learning to dance", baml_options={"on_tick": on_tick}
    )

    # Assertions
    assert result is not None
    assert tick_counts[0] > 0, f"Expected at least 1 tick, got {tick_counts[0]}"

    print(f"Total ticks: {tick_counts[0]}")
    print(f"Result: {result[:100]}...")


@pytest.mark.asyncio
async def test_ontick_stream():
    """Test on_tick with streaming function call"""
    flush()  # Clear any existing state

    on_tick, tick_events, tick_counts = get_on_tick()

    # Call streaming function with on_tick
    stream = b.stream.TestThinking(
        input="Write a story about a magical forest", baml_options={"on_tick": on_tick}
    )

    # Consume the stream
    msgs = []
    async for msg in stream:
        msgs.append(msg)

    final_result = await stream.get_final_response()

    # Assertions
    assert final_result is not None
    assert tick_counts[0] > 5, f"Expected more than 5 ticks, got {tick_counts[0]}"

    # Check if thinking content was captured
    last_thinking = tick_events[-1][1] if tick_events else ""
    assert len(last_thinking) > 0, "Expected thinking content to be captured"

    print(f"Total ticks: {tick_counts[0]}")
    print(f"Thinking content length: {len(last_thinking)}")
    print(f"Stream messages: {len(msgs)}")


@pytest.mark.asyncio
async def test_ontick_with_collector():
    """Test that on_tick creates collector automatically"""
    from baml_py import Collector

    flush()

    on_tick, tick_events, tick_counts = get_on_tick()

    # Manually create a collector to track alongside on_tick
    manual_collector = Collector("manual-collector")

    # Call with both on_tick and manual collector
    result = await b.TestThinking(
        input="Write a story about a magical forest",
        baml_options={"on_tick": on_tick, "collector": manual_collector},
    )

    # Verify both worked
    assert result is not None
    assert tick_counts[0] > 0

    # Check manual collector has logs
    logs = manual_collector.logs
    assert len(logs) > 0

    # The on_tick should have access to the same data
    assert tick_events, "on_tick should have received events"


@pytest.mark.asyncio
async def test_ontick_simple():
    """Basic test of on_tick mechanism with simple function"""
    flush()

    tick_count = [0]

    def simple_on_tick(reason: str, log: baml_py.FunctionLog):
        tick_count[0] += 1
        print(f"on_tick called! count={tick_count[0]}, reason={reason}, log={log}")

    result = await b.TestAnthropicShorthand(
        input="Hello world", baml_options={"on_tick": simple_on_tick}
    )

    assert result is not None
    print(f"Simple test - Total ticks: {tick_count[0]}")
    print(f"Result length: {len(result)}")

    assert tick_count[0] > 0, f"Expected at least 1 tick, got {tick_count[0]}"


@pytest.mark.asyncio
async def test_ontick_without_thinking():
    """Test on_tick with a function that doesn't use thinking"""
    flush()

    on_tick, tick_events, tick_counts = get_on_tick()

    # Use a simpler function that doesn't have thinking enabled
    result = await b.TestAnthropicShorthand(
        input="Hello world", baml_options={"on_tick": on_tick}
    )

    # Should still get ticks even without thinking
    assert result is not None
    assert (
        tick_counts[0] > 0
    ), f"Expected ticks even without thinking, got {tick_counts[0]}"

    print(f"Total ticks without thinking: {tick_counts[0]}")


@pytest.mark.asyncio
async def test_ontick_error_handling():
    """Test on_tick behavior when callback raises an error"""
    flush()

    tick_counts = [0]

    def on_tick_with_error(reason: str, log):
        tick_counts[0] += 1
        if tick_counts[0] == 5:
            raise ValueError("Intentional error in on_tick")

    # The function should still complete even if on_tick raises
    result = await b.TestAnthropicShorthand(
        input="Hello world", baml_options={"on_tick": on_tick_with_error}
    )

    # Function should complete despite callback error
    assert result is not None
    assert tick_counts[0] >= 5  # Should have reached the error point


@pytest.mark.asyncio
async def test_ontick_performance():
    """Test that on_tick doesn't significantly impact performance"""
    import time

    flush()

    # Run without on_tick
    start_no_tick = time.time()
    result_no_tick = await b.TestAnthropicShorthand(input="Hello world")
    time_no_tick = time.time() - start_no_tick

    # Run with on_tick
    on_tick, _, tick_counts = get_on_tick()
    start_with_tick = time.time()
    result_with_tick = await b.TestAnthropicShorthand(
        input="Hello world", baml_options={"on_tick": on_tick}
    )
    time_with_tick = time.time() - start_with_tick

    # Results should be similar
    assert result_no_tick is not None
    assert result_with_tick is not None

    # Performance should be within reasonable bounds (allowing 50% overhead)
    assert (
        time_with_tick < time_no_tick * 1.5
    ), f"on_tick overhead too high: {time_with_tick:.2f}s vs {time_no_tick:.2f}s"

    print(f"Time without on_tick: {time_no_tick:.2f}s")
    print(f"Time with on_tick: {time_with_tick:.2f}s")
    print(f"Overhead: {((time_with_tick - time_no_tick) / time_no_tick * 100):.1f}%")
    print(f"Total ticks: {tick_counts[0]}")
