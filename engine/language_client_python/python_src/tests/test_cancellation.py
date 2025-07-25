"""
Comprehensive tests for BAML Python client cancellation functionality
"""

import asyncio
import pytest
import threading
import time
from unittest.mock import Mock, MagicMock

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

from baml_py.stream import BamlStream, BamlSyncStream


class TestBamlStreamCancellation:
    """Test async BamlStream cancellation functionality"""
    
    def setup_method(self):
        """Set up test fixtures"""
        self.mock_ffi_stream = Mock()
        self.mock_ffi_stream.cancel = Mock()
        self.mock_ffi_stream.is_cancelled = Mock(return_value=False)
        self.mock_ffi_stream.on_event = Mock()
        self.mock_ffi_stream.done = Mock()
        
        self.mock_ctx_manager = Mock()
        
        def partial_coerce(result):
            return {"partial": result}
        
        def final_coerce(result):
            return {"final": result}
        
        self.stream = BamlStream(
            self.mock_ffi_stream,
            partial_coerce,
            final_coerce,
            self.mock_ctx_manager
        )
    
    def test_cancel_method_exists(self):
        """Test that cancel method exists and is callable"""
        assert hasattr(self.stream, 'cancel')
        assert callable(self.stream.cancel)
    
    def test_cancel_calls_ffi_stream_cancel(self):
        """Test that cancel() calls the FFI stream's cancel method"""
        self.stream.cancel()
        
        self.mock_ffi_stream.cancel.assert_called_once()
        assert self.stream._cancelled is True
    
    def test_is_cancelled_method(self):
        """Test is_cancelled method functionality"""
        # Initially not cancelled
        assert not self.stream.is_cancelled()
        
        # After cancellation
        self.stream.cancel()
        assert self.stream.is_cancelled()
    
    def test_is_cancelled_checks_ffi_stream(self):
        """Test that is_cancelled also checks FFI stream status"""
        self.mock_ffi_stream.is_cancelled.return_value = True
        
        assert self.stream.is_cancelled()
        self.mock_ffi_stream.is_cancelled.assert_called()
    
    @pytest.mark.asyncio
    async def test_get_final_response_with_cancellation(self):
        """Test get_final_response raises error when cancelled"""
        self.stream.cancel()
        
        # Mock the FFI stream to raise an exception
        self.mock_ffi_stream.done.side_effect = Exception("Stream cancelled")
        
        with pytest.raises(RuntimeError, match="Stream was cancelled"):
            await self.stream.get_final_response()
    
    @pytest.mark.asyncio
    async def test_get_final_response_success(self):
        """Test get_final_response works when not cancelled"""
        mock_result = Mock()
        mock_result.parsed.return_value = {"message": "test"}
        self.mock_ffi_stream.done.return_value = mock_result
        
        result = await self.stream.get_final_response()
        
        assert result == {"final": {"message": "test"}}
        self.mock_ffi_stream.done.assert_called_once_with(self.mock_ctx_manager)
    
    @pytest.mark.asyncio
    async def test_async_iteration_with_cancellation(self):
        """Test async iteration stops when cancelled"""
        partial_results = []
        
        def mock_on_event(callback):
            # Simulate some partial results
            for i in range(3):
                if not self.stream.is_cancelled():
                    mock_result = Mock()
                    mock_result.parsed.return_value = {"step": i}
                    callback(mock_result)
        
        self.mock_ffi_stream.on_event.side_effect = mock_on_event
        
        # Mock final response to complete quickly
        mock_final = Mock()
        mock_final.parsed.return_value = {"final": "done"}
        self.mock_ffi_stream.done.return_value = mock_final
        
        # Cancel after short delay
        async def cancel_after_delay():
            await asyncio.sleep(0.05)
            self.stream.cancel()
        
        cancel_task = asyncio.create_task(cancel_after_delay())
        
        # Collect partial results
        async for partial in self.stream:
            partial_results.append(partial)
            if len(partial_results) >= 2:  # Limit to avoid infinite loop
                break
        
        await cancel_task
        
        # Should have collected some partial results before cancellation
        assert len(partial_results) >= 1
        assert all("partial" in result for result in partial_results)
    
    @pytest.mark.asyncio
    async def test_multiple_cancellations_safe(self):
        """Test that multiple cancel calls are safe"""
        self.stream.cancel()
        self.stream.cancel()  # Should not raise error
        self.stream.cancel()  # Should not raise error
        
        # FFI cancel should only be called once due to _cancelled flag
        self.mock_ffi_stream.cancel.assert_called_once()


class TestBamlSyncStreamCancellation:
    """Test sync BamlSyncStream cancellation functionality"""
    
    def setup_method(self):
        """Set up test fixtures"""
        self.mock_ffi_stream = Mock()
        self.mock_ffi_stream.cancel = Mock()
        self.mock_ffi_stream.is_cancelled = Mock(return_value=False)
        self.mock_ffi_stream.on_event = Mock()
        self.mock_ffi_stream.done = Mock()
        
        self.mock_ctx_manager = Mock()
        
        def partial_coerce(result):
            return {"partial": result}
        
        def final_coerce(result):
            return {"final": result}
        
        self.stream = BamlSyncStream(
            self.mock_ffi_stream,
            partial_coerce,
            final_coerce,
            self.mock_ctx_manager
        )
    
    def test_sync_cancel_method(self):
        """Test sync cancel method"""
        self.stream.cancel()
        
        self.mock_ffi_stream.cancel.assert_called_once()
        assert self.stream._cancelled is True
    
    def test_sync_is_cancelled_method(self):
        """Test sync is_cancelled method"""
        assert not self.stream.is_cancelled()
        
        self.stream.cancel()
        assert self.stream.is_cancelled()
    
    def test_sync_get_final_response_with_cancellation(self):
        """Test sync get_final_response with cancellation"""
        self.stream.cancel()
        
        self.mock_ffi_stream.done.side_effect = Exception("Stream cancelled")
        
        with pytest.raises(RuntimeError, match="Stream was cancelled"):
            self.stream.get_final_response()
    
    def test_sync_get_final_response_success(self):
        """Test sync get_final_response success"""
        mock_result = Mock()
        mock_result.parsed.return_value = {"message": "test"}
        self.mock_ffi_stream.done.return_value = mock_result
        
        result = self.stream.get_final_response()
        
        assert result == {"final": {"message": "test"}}
    
    def test_sync_iteration_with_cancellation(self):
        """Test sync iteration with cancellation"""
        partial_results = []
        
        def mock_on_event(callback):
            # Simulate partial results
            for i in range(5):
                if not self.stream.is_cancelled():
                    mock_result = Mock()
                    mock_result.parsed.return_value = {"step": i}
                    callback(mock_result)
                    time.sleep(0.01)  # Small delay
        
        self.mock_ffi_stream.on_event.side_effect = mock_on_event
        
        # Mock final response
        mock_final = Mock()
        mock_final.parsed.return_value = {"final": "done"}
        self.mock_ffi_stream.done.return_value = mock_final
        
        # Cancel after short delay in another thread
        def cancel_after_delay():
            time.sleep(0.05)
            self.stream.cancel()
        
        cancel_thread = threading.Thread(target=cancel_after_delay)
        cancel_thread.start()
        
        # Collect partial results
        for partial in self.stream:
            partial_results.append(partial)
            if len(partial_results) >= 3:  # Limit to avoid infinite loop
                break
        
        cancel_thread.join()
        
        # Should have collected some results
        assert len(partial_results) >= 1


class TestCancellationIntegration:
    """Integration tests for cancellation functionality"""
    
    @pytest.mark.asyncio
    async def test_cancellation_prevents_resource_waste(self):
        """Test that cancellation prevents resource waste"""
        mock_ffi_stream = Mock()
        mock_ffi_stream.cancel = Mock()
        mock_ffi_stream.is_cancelled = Mock(return_value=False)
        mock_ffi_stream.on_event = Mock()
        
        # Mock a slow operation
        async def slow_done(ctx_manager):
            await asyncio.sleep(1.0)  # Simulate slow operation
            return Mock()
        
        mock_ffi_stream.done = slow_done
        
        stream = BamlStream(
            mock_ffi_stream,
            lambda x: x,
            lambda x: x,
            Mock()
        )
        
        # Start the operation and cancel quickly
        start_time = time.time()
        
        async def quick_cancel():
            await asyncio.sleep(0.1)
            stream.cancel()
        
        cancel_task = asyncio.create_task(quick_cancel())
        
        with pytest.raises(RuntimeError, match="Stream was cancelled"):
            await stream.get_final_response()
        
        await cancel_task
        elapsed = time.time() - start_time
        
        # Should complete quickly due to cancellation
        assert elapsed < 0.5, f"Cancellation took too long: {elapsed}s"
        mock_ffi_stream.cancel.assert_called_once()
    
    def test_thread_safety_of_cancellation(self):
        """Test that cancellation is thread-safe"""
        mock_ffi_stream = Mock()
        mock_ffi_stream.cancel = Mock()
        mock_ffi_stream.is_cancelled = Mock(return_value=False)
        
        stream = BamlSyncStream(
            mock_ffi_stream,
            lambda x: x,
            lambda x: x,
            Mock()
        )
        
        # Cancel from multiple threads simultaneously
        def cancel_stream():
            stream.cancel()
        
        threads = []
        for _ in range(10):
            thread = threading.Thread(target=cancel_stream)
            threads.append(thread)
            thread.start()
        
        for thread in threads:
            thread.join()
        
        # Should be cancelled and FFI cancel should be called
        assert stream.is_cancelled()
        mock_ffi_stream.cancel.assert_called()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
