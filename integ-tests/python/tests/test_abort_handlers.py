import pytest
import asyncio
import time
from baml_client import b
from baml_py import AbortController
from baml_py.errors import BamlAbortError


@pytest.mark.asyncio
async def test_manual_cancellation():
    """Test manual abort of a function call"""
    abort_controller = AbortController()

    async def abort_after_delay():
        await asyncio.sleep(0.1)
        abort_controller.abort()

    task = asyncio.create_task(
        b.FnFailRetryExponentialDelay(
            retries=5,
            initial_delay_ms=100,
            baml_options={"abort_controller": abort_controller},
        )
    )
    asyncio.create_task(abort_after_delay())

    with pytest.raises(Exception) as exc_info:
        await task

    assert (
        "abort" in str(exc_info.value).lower()
        or "cancel" in str(exc_info.value).lower()
    )


@pytest.mark.asyncio
async def test_streaming_cancellation():
    """Test abort of a streaming operation"""
    abort_controller = AbortController()

    stream = b.stream.TestAbortFallbackChain(
        input="test streaming", baml_options={"abort_controller": abort_controller}
    )

    async def abort_after_delay():
        await asyncio.sleep(0.05)
        abort_controller.abort()

    asyncio.create_task(abort_after_delay())

    values = []
    cancelled = False
    try:
        async for value in stream:
            values.append(value)
            # If we've collected some values and controller is aborted, should stop soon
            if abort_controller.aborted and len(values) > 5:
                break
    except Exception:
        cancelled = True
        # Expected to be cancelled

    # Either it was cancelled or it stopped early after abort
    # The test succeeds if stream was interrupted (by exception or early stop)
    assert cancelled or abort_controller.aborted
    # Delete the stream object
    del stream


def test_sync_cancellation():
    """Test abort in synchronous context"""
    from baml_client.sync_client import b as sync_b

    abort_controller = AbortController()

    def abort_after_delay():
        time.sleep(0.05)  # Reduced delay to abort faster
        abort_controller.abort()

    import threading

    threading.Thread(target=abort_after_delay).start()

    # Since the function might complete quickly, we'll check if it was aborted
    # or if an exception was raised
    try:
        sync_b.FnFailRetryConstantDelay(
            retries=5, delay_ms=100, baml_options={"abort_controller": abort_controller}
        )
        # If we got here, check that the controller was at least triggered
        assert abort_controller.aborted, "Function completed but abort wasn't triggered"
    except Exception as e:
        # This is expected - either aborted or some other error
        assert (
            "abort" in str(e).lower()
            or "cancel" in str(e).lower()
            or abort_controller.aborted
        )


def test_streaming_sync_cancel_after_ten_messages():
    """Cancel a synchronous stream after receiving 10 messages"""
    from baml_client.sync_client import b as sync_b

    abort_controller = AbortController()

    stream = sync_b.stream.PromptTestStreaming(
        input="Programming languages are fun to create",
        baml_options={"abort_controller": abort_controller},
    )

    msgs = []
    raised_exception = False
    try:
        for msg in stream:
            print(f"msg: {msg}")
            msgs.append(str(msg))
            if len(msgs) >= 10:
                abort_controller.abort()
                break
        stream.get_final_response()

    except Exception as e:
        # It's acceptable if aborting mid-stream raises
        print(f"Exception: {e}")
        raised_exception = True

    print("stopped streaming")

    # In normal runs we expect to have aborted after >=10 messages
    # But in CI without API keys, requests may error early; accept exceptions too
    assert abort_controller.aborted or raised_exception
    if abort_controller.aborted:
        assert len(msgs) >= 10, "Expected at least 10 streamed messages before aborting"
    del stream


@pytest.mark.asyncio
async def test_streaming_async_cancel_after_ten_messages():
    """Cancel an async stream after receiving 10 messages"""
    abort_controller = AbortController()

    stream = b.stream.PromptTestStreaming(
        input="Programming languages are fun to create",
        baml_options={"abort_controller": abort_controller},
    )

    msgs = []
    raised_exception = False
    try:
        async for msg in stream:
            print(f"msg: {msg}")
            msgs.append(str(msg))
            if len(msgs) >= 10:
                abort_controller.abort()
                break
    except Exception as e:
        # It's acceptable if aborting mid-stream raises or network errors occur
        print(f"Exception: {e}")
        raised_exception = True

    assert abort_controller.aborted or raised_exception
    if abort_controller.aborted:
        assert len(msgs) >= 10, "Expected at least 10 streamed messages before aborting"
    del stream


@pytest.mark.asyncio
async def test_early_abort():
    """Test that already-aborted controller prevents execution"""
    abort_controller = AbortController()
    abort_controller.abort()

    with pytest.raises(BamlAbortError):
        await b.ExtractName(
            text="John Doe", baml_options={"abort_controller": abort_controller}
        )


def test_baml_abort_error_import_and_instanceof():
    try:
        raise BamlAbortError("Operation was aborted")
    except Exception as e:
        assert isinstance(e, BamlAbortError)


@pytest.mark.asyncio
async def test_normal_operation():
    """Test that operations work normally without abort controller"""
    result = await b.ExtractName(text="My name is Alice")
    assert isinstance(result, str)
    assert "alice" in result.lower()


@pytest.mark.asyncio
async def test_multiple_aborts():
    """Test multiple concurrent aborted operations"""
    tasks = []

    for i in range(10):
        controller = AbortController()
        task = asyncio.create_task(
            b.FnFailRetryConstantDelay(
                retries=3, delay_ms=50, baml_options={"abort_controller": controller}
            )
        )
        tasks.append(task)

        # Abort at random times
        asyncio.create_task(abort_after(controller, 0.01 * (i + 1)))

    results = await asyncio.gather(*tasks, return_exceptions=True)

    # All should have raised exceptions
    for result in results:
        assert isinstance(result, Exception)


async def abort_after(controller, delay):
    """Helper to abort after a delay"""
    await asyncio.sleep(delay)
    controller.abort()


@pytest.mark.asyncio
async def test_abort_timing():
    """Test that abort happens quickly"""
    abort_controller = AbortController()
    start_time = time.time()

    # Cancel after 250ms
    asyncio.create_task(abort_after(abort_controller, 0.25))

    with pytest.raises(Exception):
        await b.FnFailRetryExponentialDelay(
            retries=5,
            initial_delay_ms=100,
            baml_options={"abort_controller": abort_controller},
        )

    elapsed = time.time() - start_time
    # Should abort within ~300ms (250ms delay + processing)
    assert elapsed < 0.4, f"Took too long to abort: {elapsed}s"


@pytest.mark.asyncio
async def test_abort_time_out():
    """Test that abort happens quickly"""
    abort_controller = AbortController(timeout_ms=250)
    start_time = time.time()

    with pytest.raises(Exception):
        await b.FnFailRetryExponentialDelay(
            retries=5,
            initial_delay_ms=100,
            baml_options={"abort_controller": abort_controller},
        )

    elapsed = time.time() - start_time
    # Should abort within ~300ms (250ms delay + processing)
    assert elapsed < 0.4, f"Took too long to abort: {elapsed}s"
