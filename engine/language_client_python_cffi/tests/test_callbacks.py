import pytest
import asyncio
import threading
import time
from typing import List, Any, Optional, Callable
from unittest.mock import Mock, patch
import ctypes

from baml_py_cffi._callbacks import (
    _trigger_callback,
    _error_callback,
    _tick_callback,
    register_callbacks,
    CallbackState,
    _active_callbacks,
    _callback_lock,
)
from baml_py_cffi._result_types import ResultCallback, BamlError
from baml_py_cffi._async_utils import make_async_call


class TestCallbacks:
    """Test the callback system"""

    def setup_method(self) -> None:
        """Clean up callbacks before each test"""
        with _callback_lock:
            _active_callbacks.clear()

    def test_callback_state_creation(self) -> None:
        """Test that CallbackState can be created"""
        loop: asyncio.AbstractEventLoop = asyncio.new_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()
        state: CallbackState = CallbackState(queue=queue, loop=loop)
        assert state.queue == queue
        assert state.loop == loop
        assert state.on_tick is None
        assert state.type_map == {}

    def test_register_callbacks(self) -> None:
        """Test that callbacks can be registered with a mock library"""
        mock_lib: Mock = Mock()
        mock_lib.register_callbacks = Mock()

        register_callbacks(mock_lib)

        # Verify register_callbacks was called with 3 callback functions
        mock_lib.register_callbacks.assert_called_once()
        args: tuple = mock_lib.register_callbacks.call_args[0]
        assert len(args) == 3
        assert callable(args[0])  # trigger callback
        assert callable(args[1])  # error callback
        assert callable(args[2])  # tick callback

    @pytest.mark.asyncio
    async def test_trigger_callback_single_result(self) -> None:
        """Test trigger callback for single result"""
        # Setup
        call_id: int = 12345
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop)

        # Simulate callback with data
        test_data: bytes = b"test result data"
        # Convert to POINTER(c_int8) as expected by the callback
        data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
        _trigger_callback(call_id, 1, data_ptr, len(test_data))

        # Wait a bit for the callback to be scheduled
        await asyncio.sleep(0.1)

        # Check result
        result_cb: ResultCallback = await queue.get()
        assert isinstance(result_cb, ResultCallback)
        assert result_cb.has_data
        assert result_cb.data == test_data
        assert result_cb.error is None

        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks

    @pytest.mark.asyncio
    async def test_error_callback(self) -> None:
        """Test error callback"""
        # Setup
        call_id: int = 54321
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop)

        # Simulate error callback
        error_msg: bytes = b"Something went wrong"
        # Convert to POINTER(c_int8) as expected by the callback
        error_ptr = ctypes.cast(error_msg, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
        _error_callback(call_id, 1, error_ptr, len(error_msg))

        # Wait a bit for the callback to be scheduled
        await asyncio.sleep(0.1)

        # Check error was set
        result_cb: ResultCallback = await queue.get()
        assert isinstance(result_cb, ResultCallback)
        assert result_cb.error is not None
        assert isinstance(result_cb.error, BamlError)
        assert "Something went wrong" in str(result_cb.error)

        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks

    @pytest.mark.asyncio
    async def test_streaming_callback(self) -> None:
        """Test trigger callback for streaming results"""
        # Setup
        call_id: int = 99999
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop)

        # Simulate multiple streaming callbacks
        chunks: List[bytes] = [b"chunk1", b"chunk2", b"chunk3"]
        for i, chunk in enumerate(chunks):
            is_done: int = 0 if i < len(chunks) - 1 else 1
            # Convert to POINTER(c_int8) as expected by the callback
            chunk_ptr = ctypes.cast(chunk, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
            _trigger_callback(call_id, is_done, chunk_ptr, len(chunk))

        # Collect results
        results: List[bytes] = []
        for _ in range(3):
            item: ResultCallback = await asyncio.wait_for(queue.get(), timeout=0.5)
            assert isinstance(item, ResultCallback)
            if item.has_stream_data:
                results.append(item.stream_data)
            elif item.has_data:
                results.append(item.data)

        # Check results
        assert results == chunks

        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks

    def test_thread_safety(self) -> None:
        """Test that callbacks are thread-safe"""
        # Create multiple threads that add/remove callbacks
        results: List[str] = []
        errors: List[str] = []

        def add_remove_callbacks() -> None:
            loops_to_close: List[asyncio.AbstractEventLoop] = []
            try:
                for i in range(100):
                    call_id: int = threading.get_ident() * 1000 + i
                    loop: asyncio.AbstractEventLoop = asyncio.new_event_loop()
                    loops_to_close.append(loop)
                    asyncio.set_event_loop(loop)  # Set the loop for this thread
                    queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

                    # Add callback
                    with _callback_lock:
                        _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop)

                    # Remove callback
                    with _callback_lock:
                        _active_callbacks.pop(call_id, None)

                results.append("success")
            except Exception as e:
                errors.append(str(e))
            finally:
                # Clean up event loops
                for loop in loops_to_close:
                    loop.close()

        threads: List[threading.Thread] = []
        for _ in range(10):
            t: threading.Thread = threading.Thread(target=add_remove_callbacks)
            threads.append(t)
            t.start()

        for t in threads:
            t.join()

        # Verify no errors and all succeeded
        assert len(errors) == 0
        assert len(results) == 10
        assert all(r == "success" for r in results)

        # Clean up any leftover callbacks from this test
        with _callback_lock:
            # Count callbacks from this test
            test_callbacks: List[int] = [
                k for k in _active_callbacks.keys() if k >= threading.get_ident() * 1000
            ]
            assert len(test_callbacks) == 0

    @pytest.mark.asyncio
    async def test_make_async_call_success(self) -> None:
        """Test make_async_call with successful result"""

        # Mock call function
        def mock_call_fn(*args: Any) -> Optional[bytes]:
            call_id: int = args[-1]
            # Get the current event loop to schedule callback properly
            loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()

            # Simulate async callback after a short delay
            async def simulate_callback() -> None:
                await asyncio.sleep(0.01)  # Short delay
                test_data: bytes = b"async result"
                # Convert to POINTER(c_int8) as expected by the callback
                data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]  # type: ignore[arg-type]
                _trigger_callback(call_id, 1, data_ptr, len(test_data))

            # Schedule the callback coroutine
            asyncio.ensure_future(simulate_callback(), loop=loop)
            return None  # No error

        # Make async call (now uses queue-based approach internally)
        result: Any = await make_async_call(mock_call_fn, "arg1", "arg2")
        assert result == b"async result"

    @pytest.mark.asyncio
    async def test_make_async_call_sync_error(self) -> None:
        """Test make_async_call with synchronous error"""

        # Mock call function that returns error
        def mock_call_fn(*args: Any) -> bytes:
            return b"Synchronous error occurred"

        # Make async call (now uses queue-based approach internally)
        with pytest.raises(RuntimeError) as exc_info:
            await make_async_call(mock_call_fn, "arg1", "arg2")
        assert "Call failed: Synchronous error occurred" in str(exc_info.value)

    @pytest.mark.asyncio
    async def test_make_async_call_cleanup_on_exception(self) -> None:
        """Test that callbacks are cleaned up on exception"""

        # Mock call function that raises exception
        def mock_call_fn(*args: Any) -> None:
            raise ValueError("Test exception")

        # Make async call (now uses queue-based approach internally)
        with pytest.raises(ValueError):
            await make_async_call(mock_call_fn, "arg1", "arg2")

        # Verify callbacks are cleaned up
        with _callback_lock:
            assert len(_active_callbacks) == 0
