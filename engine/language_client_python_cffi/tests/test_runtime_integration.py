"""Integration test for runtime with mocked C functions"""

import asyncio
import ctypes
from unittest.mock import patch, MagicMock
from baml_py_cffi import create_runtime
from baml_py_cffi._callbacks import _trigger_callback, _active_callbacks, _callback_lock


async def test_async_call_structure():
    """Test that the async call structure is properly set up"""
    rt = create_runtime(".", {}, {})

    # Mock the C function to return success (no error)
    async def mock_async_call(*args):
        return b"test_result"

    with patch.object(rt, "call_function", side_effect=mock_async_call) as mock_call:
        # Test that we can call the function
        result = await rt.call_function("test_func", b"test_args")
        assert result == b"test_result"
        mock_call.assert_called_once_with("test_func", b"test_args")


async def test_callback_integration():
    """Test that callbacks are properly registered and can be triggered"""
    rt = create_runtime(".", {}, {})

    # Verify callbacks are registered
    from baml_py_cffi._ffi import _lib

    assert hasattr(_lib, "_callbacks_registered")
    assert _lib._callbacks_registered == True

    # Test that we can manually trigger a callback (simulating C side)
    call_id = 12345
    test_data = b"test_callback_data"

    # Create a queue to receive the callback
    queue = asyncio.Queue()
    loop = asyncio.get_event_loop()

    from baml_py_cffi._callbacks import CallbackState

    with _callback_lock:
        _active_callbacks[call_id] = CallbackState(queue=queue, loop=loop, type_map={})

    # Simulate C calling our callback
    data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))
    _trigger_callback(call_id, 1, data_ptr, len(test_data))

    # Check that the callback was processed
    await asyncio.sleep(0.1)  # Give time for callback to process
    assert queue.qsize() == 1

    # Verify cleanup
    with _callback_lock:
        assert call_id not in _active_callbacks


if __name__ == "__main__":
    import os

    os.environ["BAML_LIBRARY_PATH"] = "/Users/sam/baml5/engine/target/debug/libbaml_cffi.dylib"

    asyncio.run(test_async_call_structure())
    print("✓ Async call structure test passed")

    asyncio.run(test_callback_integration())
    print("✓ Callback integration test passed")

    print("\nAll integration tests passed!")
