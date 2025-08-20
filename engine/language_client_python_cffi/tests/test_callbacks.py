import pytest
import asyncio
import threading
import time
from unittest.mock import Mock, patch
import ctypes

from baml_py_cffi._callbacks import (
    _trigger_callback,
    _error_callback,
    _tick_callback,
    register_callbacks,
    CallbackState,
    _active_callbacks,
    _callback_lock
)
from baml_py_cffi._async_utils import make_async_call


class TestCallbacks:
    """Test the callback system"""
    
    def setup_method(self):
        """Clean up callbacks before each test"""
        with _callback_lock:
            _active_callbacks.clear()
    
    def test_callback_state_creation(self):
        """Test that CallbackState can be created"""
        loop = asyncio.new_event_loop()
        future = loop.create_future()
        state = CallbackState(future=future, loop=loop)
        assert state.future == future
        assert state.loop == loop
        assert state.queue is None
        assert state.on_tick is None
    
    def test_register_callbacks(self):
        """Test that callbacks can be registered with a mock library"""
        mock_lib = Mock()
        mock_lib.register_callbacks = Mock()
        
        register_callbacks(mock_lib)
        
        # Verify register_callbacks was called with 3 callback functions
        mock_lib.register_callbacks.assert_called_once()
        args = mock_lib.register_callbacks.call_args[0]
        assert len(args) == 3
        assert callable(args[0])  # trigger callback
        assert callable(args[1])  # error callback
        assert callable(args[2])  # tick callback
    
    @pytest.mark.asyncio
    async def test_trigger_callback_single_result(self):
        """Test trigger callback for single result"""
        # Setup
        call_id = 12345
        loop = asyncio.get_event_loop()
        future = loop.create_future()
        
        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(
                future=future,
                loop=loop
            )
        
        # Simulate callback with data
        test_data = b"test result data"
        # Convert to POINTER(c_int8) as expected by the callback
        data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))
        _trigger_callback(call_id, 1, data_ptr, len(test_data))
        
        # Wait a bit for the callback to be scheduled
        await asyncio.sleep(0.1)
        
        # Check result
        assert future.done()
        result = future.result()
        assert result == test_data
        
        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks
    
    @pytest.mark.asyncio
    async def test_error_callback(self):
        """Test error callback"""
        # Setup
        call_id = 54321
        loop = asyncio.get_event_loop()
        future = loop.create_future()
        
        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(
                future=future,
                loop=loop
            )
        
        # Simulate error callback
        error_msg = b"Something went wrong"
        # Convert to POINTER(c_int8) as expected by the callback
        error_ptr = ctypes.cast(error_msg, ctypes.POINTER(ctypes.c_int8))
        _error_callback(call_id, 1, error_ptr, len(error_msg))
        
        # Wait a bit for the callback to be scheduled
        await asyncio.sleep(0.1)
        
        # Check error was set
        assert future.done()
        with pytest.raises(RuntimeError) as exc_info:
            future.result()
        assert "BAML Error: Something went wrong" in str(exc_info.value)
        
        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks
    
    @pytest.mark.asyncio
    async def test_streaming_callback(self):
        """Test trigger callback for streaming results"""
        # Setup
        call_id = 99999
        loop = asyncio.get_event_loop()
        future = loop.create_future()
        queue = asyncio.Queue()
        
        with _callback_lock:
            _active_callbacks[call_id] = CallbackState(
                future=future,
                queue=queue,
                loop=loop
            )
        
        # Simulate multiple streaming callbacks
        chunks = [b"chunk1", b"chunk2", b"chunk3"]
        for i, chunk in enumerate(chunks):
            is_done = 0 if i < len(chunks) - 1 else 1
            # Convert to POINTER(c_int8) as expected by the callback
            chunk_ptr = ctypes.cast(chunk, ctypes.POINTER(ctypes.c_int8))
            _trigger_callback(call_id, is_done, chunk_ptr, len(chunk))
        
        # Collect results
        results = []
        while True:
            try:
                item = await asyncio.wait_for(queue.get(), timeout=0.5)
                if item is None:
                    break
                results.append(item)
            except asyncio.TimeoutError:
                break
        
        # Check results
        assert results == chunks
        
        # Verify cleanup
        with _callback_lock:
            assert call_id not in _active_callbacks
    
    def test_thread_safety(self):
        """Test that callbacks are thread-safe"""
        # Create multiple threads that add/remove callbacks
        results = []
        errors = []
        
        def add_remove_callbacks():
            loops_to_close = []
            try:
                for i in range(100):
                    call_id = threading.get_ident() * 1000 + i
                    loop = asyncio.new_event_loop()
                    loops_to_close.append(loop)
                    future = loop.create_future()
                    
                    # Add callback
                    with _callback_lock:
                        _active_callbacks[call_id] = CallbackState(
                            future=future,
                            loop=loop
                        )
                    
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
        
        threads = []
        for _ in range(10):
            t = threading.Thread(target=add_remove_callbacks)
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
            test_callbacks = [k for k in _active_callbacks.keys() if k >= threading.get_ident() * 1000]
            assert len(test_callbacks) == 0
    
    @pytest.mark.asyncio
    async def test_make_async_call_success(self):
        """Test make_async_call with successful result"""
        # Mock call function
        def mock_call_fn(*args):
            call_id = args[-1]
            # Get the current event loop to schedule callback properly
            loop = asyncio.get_event_loop()
            
            # Simulate async callback after a short delay
            async def simulate_callback():
                await asyncio.sleep(0.01)  # Short delay
                test_data = b"async result"
                # Convert to POINTER(c_int8) as expected by the callback
                data_ptr = ctypes.cast(test_data, ctypes.POINTER(ctypes.c_int8))
                _trigger_callback(call_id, 1, data_ptr, len(test_data))
            
            # Schedule the callback coroutine
            asyncio.ensure_future(simulate_callback(), loop=loop)
            return None  # No error
        
        # Make async call
        result = await make_async_call(mock_call_fn, "arg1", "arg2")
        assert result == b"async result"
    
    @pytest.mark.asyncio
    async def test_make_async_call_sync_error(self):
        """Test make_async_call with synchronous error"""
        # Mock call function that returns error
        def mock_call_fn(*args):
            return b"Synchronous error occurred"
        
        # Make async call
        with pytest.raises(RuntimeError) as exc_info:
            await make_async_call(mock_call_fn, "arg1", "arg2")
        assert "Call failed: Synchronous error occurred" in str(exc_info.value)
    
    @pytest.mark.asyncio
    async def test_make_async_call_cleanup_on_exception(self):
        """Test that callbacks are cleaned up on exception"""
        # Mock call function that raises exception
        def mock_call_fn(*args):
            raise ValueError("Test exception")
        
        # Make async call
        with pytest.raises(ValueError):
            await make_async_call(mock_call_fn, "arg1", "arg2")
        
        # Verify callbacks are cleaned up
        with _callback_lock:
            assert len(_active_callbacks) == 0