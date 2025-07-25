"""
BAML Stream with cancellation support for Python
"""

import asyncio
from typing import TypeVar, Generic, Optional, Callable, Iterator, AsyncIterator
import threading

T = TypeVar('T')
PartialT = TypeVar('PartialT')
FinalT = TypeVar('FinalT')


class BamlStream(Generic[PartialT, FinalT]):
    """
    A BAML stream that supports cancellation.
    
    This stream can be cancelled to stop ongoing HTTP requests to LLM providers,
    preventing wasted resources and billing charges.
    """
    
    def __init__(
        self,
        ffi_stream,
        partial_coerce: Callable[[any], PartialT],
        final_coerce: Callable[[any], FinalT],
        ctx_manager,
    ):
        self._ffi_stream = ffi_stream
        self._partial_coerce = partial_coerce
        self._final_coerce = final_coerce
        self._ctx_manager = ctx_manager
        self._cancelled = False
        self._final_result = None
        
    def cancel(self) -> None:
        """
        Cancel the stream processing.
        
        This will:
        1. Cancel the Rust-level stream
        2. Cancel ongoing HTTP requests to LLM providers
        3. Stop consuming network bandwidth and API quota
        4. Clean up resources
        """
        if not self._cancelled:
            self._cancelled = True
            self._ffi_stream.cancel()
    
    def is_cancelled(self) -> bool:
        """Check if the stream has been cancelled."""
        return self._cancelled or self._ffi_stream.is_cancelled()
    
    async def get_final_response(self) -> FinalT:
        """
        Get the final response from the stream.
        
        This will wait for the stream to complete and return the final result.
        If the stream is cancelled, this will raise an exception.
        """
        if self._final_result is not None:
            return self._final_result
            
        try:
            result = await self._ffi_stream.done(self._ctx_manager)
            final_result = self._final_coerce(result.parsed())
            self._final_result = final_result
            return final_result
        except Exception as e:
            if self.is_cancelled():
                raise RuntimeError("Stream was cancelled") from e
            raise
    
    def __aiter__(self) -> AsyncIterator[PartialT]:
        """Async iterator support for streaming partial results."""
        return self._async_iter()
    
    async def _async_iter(self) -> AsyncIterator[PartialT]:
        """Internal async iterator implementation."""
        partial_results = []
        
        def on_event(result):
            if not self.is_cancelled():
                try:
                    partial = self._partial_coerce(result.parsed())
                    partial_results.append(partial)
                except Exception as e:
                    # Log error but continue streaming
                    print(f"Error processing partial result: {e}")
        
        # Set up event handler
        self._ffi_stream.on_event(on_event)
        
        # Start the stream processing
        stream_task = asyncio.create_task(self.get_final_response())
        
        try:
            # Yield partial results as they come in
            last_index = 0
            while not stream_task.done() and not self.is_cancelled():
                # Yield any new partial results
                while last_index < len(partial_results):
                    yield partial_results[last_index]
                    last_index += 1
                
                # Small delay to avoid busy waiting
                await asyncio.sleep(0.01)
            
            # Yield any remaining partial results
            while last_index < len(partial_results):
                yield partial_results[last_index]
                last_index += 1
                
            # Wait for final result (or cancellation)
            await stream_task
            
        except Exception as e:
            if self.is_cancelled():
                raise RuntimeError("Stream was cancelled") from e
            raise
        finally:
            # Clean up event handler
            self._ffi_stream.on_event(None)


class BamlSyncStream(Generic[PartialT, FinalT]):
    """
    Synchronous version of BamlStream with cancellation support.
    """
    
    def __init__(
        self,
        ffi_stream,
        partial_coerce: Callable[[any], PartialT],
        final_coerce: Callable[[any], FinalT],
        ctx_manager,
    ):
        self._ffi_stream = ffi_stream
        self._partial_coerce = partial_coerce
        self._final_coerce = final_coerce
        self._ctx_manager = ctx_manager
        self._cancelled = False
        self._final_result = None
        
    def cancel(self) -> None:
        """
        Cancel the stream processing.
        
        This will:
        1. Cancel the Rust-level stream
        2. Cancel ongoing HTTP requests to LLM providers
        3. Stop consuming network bandwidth and API quota
        4. Clean up resources
        """
        if not self._cancelled:
            self._cancelled = True
            self._ffi_stream.cancel()
    
    def is_cancelled(self) -> bool:
        """Check if the stream has been cancelled."""
        return self._cancelled or self._ffi_stream.is_cancelled()
    
    def get_final_response(self) -> FinalT:
        """
        Get the final response from the stream.
        
        This will block until the stream completes and return the final result.
        If the stream is cancelled, this will raise an exception.
        """
        if self._final_result is not None:
            return self._final_result
            
        try:
            result = self._ffi_stream.done(self._ctx_manager)
            final_result = self._final_coerce(result.parsed())
            self._final_result = final_result
            return final_result
        except Exception as e:
            if self.is_cancelled():
                raise RuntimeError("Stream was cancelled") from e
            raise
    
    def __iter__(self) -> Iterator[PartialT]:
        """Iterator support for streaming partial results."""
        return self._sync_iter()
    
    def _sync_iter(self) -> Iterator[PartialT]:
        """Internal sync iterator implementation."""
        partial_results = []
        stream_done = threading.Event()
        
        def on_event(result):
            if not self.is_cancelled():
                try:
                    partial = self._partial_coerce(result.parsed())
                    partial_results.append(partial)
                except Exception as e:
                    # Log error but continue streaming
                    print(f"Error processing partial result: {e}")
        
        # Set up event handler
        self._ffi_stream.on_event(on_event)
        
        # Start stream processing in background thread
        def run_stream():
            try:
                self.get_final_response()
            except Exception:
                pass  # Error will be handled when get_final_response is called again
            finally:
                stream_done.set()
        
        stream_thread = threading.Thread(target=run_stream)
        stream_thread.start()
        
        try:
            # Yield partial results as they come in
            last_index = 0
            while not stream_done.is_set() and not self.is_cancelled():
                # Yield any new partial results
                while last_index < len(partial_results):
                    yield partial_results[last_index]
                    last_index += 1
                
                # Small delay to avoid busy waiting
                stream_done.wait(0.01)
            
            # Yield any remaining partial results
            while last_index < len(partial_results):
                yield partial_results[last_index]
                last_index += 1
                
        finally:
            # Clean up
            self._ffi_stream.on_event(None)
            stream_thread.join(timeout=1.0)  # Give it a second to clean up
