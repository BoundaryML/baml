import pytest
import asyncio
import ctypes
from typing import List, Any
from unittest.mock import patch
from baml_py_cffi._callbacks import (
    _trigger_callback,
    _error_callback,
    _active_callbacks,
    _callback_lock,
    CallbackState,
)
from baml_py_cffi._result_types import ResultCallback, BamlError


class TestQueueCallbacks:
    """Test queue-based callback system"""

    def setup_method(self) -> None:
        """Clean up callbacks before each test"""
        with _callback_lock:
            _active_callbacks.clear()

    @pytest.mark.asyncio
    async def test_trigger_callback_single_result(self) -> None:
        """Test callback with single result using queue"""
        call_id: int = 12345
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop, type_map={})

        # Create mock data
        test_data: bytes = b"mock_encoded_data"
        data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]

        _trigger_callback(call_id, 1, data_ptr, len(test_data))

        # Use wrapper to get single result from queue
        from baml_py_cffi._async_utils import queue_to_single_result

        result: Any = await asyncio.wait_for(queue_to_single_result(queue), timeout=0.5)
        assert result == test_data  # Currently returns raw data

        # Verify cleanup happened
        with _callback_lock:
            assert call_id not in _active_callbacks

    @pytest.mark.asyncio
    async def test_streaming_with_queue(self) -> None:
        """Test streaming with queue-based approach"""
        call_id: int = 99999
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop, type_map={})

        # Stream multiple values
        values: List[bytes] = [b"chunk1", b"chunk2", b"chunk3"]
        for i, value in enumerate(values):
            is_done: int = 1 if i == len(values) - 1 else 0
            data_ptr = ctypes.cast(value, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
            _trigger_callback(call_id, is_done, data_ptr, len(value))

        # Collect results
        results: List[bytes] = []
        for _ in range(3):
            item: ResultCallback = await asyncio.wait_for(queue.get(), timeout=0.5)
            assert isinstance(item, ResultCallback)
            if item.has_stream_data:
                results.append(item.stream_data)
            elif item.has_data:
                results.append(item.data)

        assert results == values

    @pytest.mark.asyncio
    async def test_error_handling_through_queue(self) -> None:
        """Test error handling through queue-based approach"""
        call_id: int = 77777
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop, type_map={})

        # Simulate error callback
        error_msg: bytes = b"Test error message"
        error_ptr = ctypes.cast(error_msg, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
        _error_callback(call_id, 1, error_ptr, len(error_msg))

        # Get error from queue
        result_cb: ResultCallback = await asyncio.wait_for(queue.get(), timeout=0.5)
        assert isinstance(result_cb, ResultCallback)
        assert result_cb.error is not None
        assert isinstance(result_cb.error, BamlError)
        assert "Test error message" in str(result_cb.error)
        assert not result_cb.has_data
        assert not result_cb.has_stream_data

        # Verify cleanup happened
        with _callback_lock:
            assert call_id not in _active_callbacks

    @pytest.mark.asyncio
    async def test_error_during_streaming(self) -> None:
        """Test error handling during streaming operation"""
        call_id: int = 88888
        loop: asyncio.AbstractEventLoop = asyncio.get_event_loop()
        queue: asyncio.Queue[ResultCallback] = asyncio.Queue()

        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop, type_map={})

        # Send some stream data first
        chunk1: bytes = b"first chunk"
        data_ptr = ctypes.cast(chunk1, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
        _trigger_callback(call_id, 0, data_ptr, len(chunk1))  # not done

        # Then send an error
        error_msg: bytes = b"Stream interrupted"
        error_ptr = ctypes.cast(error_msg, ctypes.POINTER(ctypes.c_int8))  # type: ignore[arg-type]
        _error_callback(call_id, 1, error_ptr, len(error_msg))

        # Collect results
        results: List[bytes] = []
        errors: List[Exception] = []
        for _ in range(2):
            item: ResultCallback = await asyncio.wait_for(queue.get(), timeout=0.5)
            assert isinstance(item, ResultCallback)
            if item.error:
                errors.append(item.error)
            elif item.has_stream_data:
                results.append(item.stream_data)

        # Verify we got one chunk and one error
        assert len(results) == 1
        assert results[0] == chunk1
        assert len(errors) == 1
        assert "Stream interrupted" in str(errors[0])

        # Verify cleanup happened
        with _callback_lock:
            assert call_id not in _active_callbacks
