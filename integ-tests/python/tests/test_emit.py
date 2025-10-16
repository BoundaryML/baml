import pytest
import asyncio
from ..baml_client import b
from ..baml_client import watchers


@pytest.mark.asyncio
async def test_emit_basic_changes():
    """Test that emit events are fired correctly"""
    listener = watchers.WorkflowWatch()

    # Track if we saw changes
    saw_x_change = False
    saw_once_change = False
    saw_twice_change = False

    # Collect all events
    captured_events = []

    def on_x_change(ev):
        print("SAW X CHANGE!")
        print(ev)
        nonlocal saw_x_change
        print(f"x changed: {ev.variable_name} = {ev.value}")
        captured_events.append(("x", ev))
        saw_x_change = True

    def on_once_change(ev):
        print("SAW ONCE CHANGE!")
        print(ev)
        nonlocal saw_once_change
        print(f"once changed: {ev.variable_name} = {ev.value}")
        captured_events.append(("once", ev))
        saw_once_change = True

    def on_twice_change(ev):
        print("SAW TWICE CHANGE!")
        nonlocal saw_twice_change
        print(f"twice changed: {ev.variable_name} = {ev.value}")
        captured_events.append(("twice", ev))
        saw_twice_change = True

    def on_sub_x_change(ev):
        print("SAW SUB_X CHANGE!!!!!!!!!!!!!")

    # Register event handlers
    # listener.on_var("x", lambda x: print(x))
    listener.on_var("x", on_x_change)
    listener.on_var("once", on_once_change)
    listener.on_var("twice", on_twice_change)

    listener.function_WorkflowWatchChild.on_var("x", lambda ev: on_sub_x_change(ev))

    # Call the function with the event listener
    response = await b.WorkflowWatch({"watchers": listener})

    # Give some time for events to be processed
    await asyncio.sleep(0.5)

    # Verify we saw the changes
    assert saw_x_change, "Should have seen x variable change"
    assert saw_once_change, "Should have seen once variable change"
    assert saw_twice_change, "Should have seen twice variable change"

    # Print captured events for debugging
    print(f"\nCaptured {len(captured_events)} events:")
    for channel, event in captured_events:
        print(
            f"  {channel}: {event.variable_name} = {event.value} at {event.timestamp}"
        )

    print(f"\nFunction result: {response}")


@pytest.mark.asyncio
async def test_emit_stream_handler():
    """Test that stream handlers work correctly"""
    listener = watchers.WorkflowWatch()

    stream_chunks = []

    def on_x_stream(stream_event):
        print(f"Stream chunk: {stream_event.variable_name} = {stream_event.value}")
        stream_chunks.append(stream_event)

    listener.on_stream("x", on_x_stream)

    # Call the function with the event listener
    _ = await b.WorkflowWatch({"watchers": listener})

    # Give some time for events to be processed
    await asyncio.sleep(0.5)

    # Verify we received stream chunks
    print(f"\nReceived {len(stream_chunks)} stream chunks")
    for chunk in stream_chunks:
        print(f"  {chunk.variable_name} = {chunk.value} at {chunk.timestamp}")


@pytest.mark.asyncio
async def test_emit_block_handler():
    """Test that block handlers work correctly"""
    listener = watchers.WorkflowWatch()

    block_events = []

    def on_block(event):
        print(f"Block event: {event.block_label} - {event.event_type}")
        block_events.append(event)

    listener.on_block(on_block)

    # Call the function with the event listener
    _ = await b.WorkflowWatch({"watchers": listener})

    # Give some time for events to be processed
    await asyncio.sleep(0.5)

    # Print block events for debugging
    print(f"\nReceived {len(block_events)} block events:")
    for event in block_events:
        print(f"  {event.block_label}: {event.event_type}")


# @pytest.mark.asyncio
# async def test_emit_child_function():
#     """Test that child function events work correctly"""
#     listener = watchers.WorkflowWatch()
#
#     child_events = []
#
#     def on_child_x(ev):
#         print(f"Child x changed: {ev.variable_name} = {ev.value}")
#         child_events.append(ev)
#
#     # Register handler for child function's variable
#     listener.function_WorkflowWatchChild.on_var("x", on_child_x)
#
#     # Call the function with the event listener
#     _ = await b.WorkflowEmit({"watchers": listener})
#
#     # Give some time for events to be processed
#     await asyncio.sleep(0.5)
#
#     # Verify we saw child function events
#     assert len(child_events) > 0, "Should have seen child function variable changes"
#
#     print(f"\nReceived {len(child_events)} child function events:")
#     for event in child_events:
#         print(f"  {event.function_name}.{event.variable_name} = {event.value}")


@pytest.mark.asyncio
async def test_emit_multiple_handlers():
    """Test that multiple handlers on the same channel work correctly"""
    listener = watchers.WorkflowWatch()

    handler1_calls = []
    handler2_calls = []

    def handler1(ev):
        handler1_calls.append(ev)

    def handler2(ev):
        handler2_calls.append(ev)

    # Register multiple handlers for the same variable
    listener.on_var("x", handler1)
    listener.on_var("x", handler2)

    # Call the function with the event listener
    _ = await b.WorkflowWatch({"watchers": listener})

    # Give some time for events to be processed
    await asyncio.sleep(0.5)

    # Both handlers should have been called
    assert len(handler1_calls) > 0, "Handler 1 should have been called"
    assert len(handler2_calls) > 0, "Handler 2 should have been called"
    assert len(handler1_calls) == len(handler2_calls), (
        "Both handlers should be called same number of times"
    )

    print(f"\nHandler 1 called {len(handler1_calls)} times")
    print(f"Handler 2 called {len(handler2_calls)} times")
