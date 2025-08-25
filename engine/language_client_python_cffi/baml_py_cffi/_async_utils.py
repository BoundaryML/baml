import asyncio
import random
from typing import Optional, Any, AsyncIterator, TypeVar
from ._callbacks import CallbackState, _active_callbacks, _callback_lock
from ._result_types import ResultCallback

T = TypeVar('T')

async def queue_to_single_result(queue: asyncio.Queue[ResultCallback]) -> T:
    """Convert a queue-based callback to a single result"""
    result_cb = await queue.get()
    
    if result_cb.error:
        raise result_cb.error
    
    # For single results, we expect has_data to be True
    if result_cb.has_data:
        return result_cb.data
    
    # Handle edge case - shouldn't happen in normal flow
    raise RuntimeError("Expected single result but got stream data or empty result")

async def queue_to_stream(queue: asyncio.Queue[ResultCallback]) -> AsyncIterator[T]:
    """Convert a queue-based callback to an async iterator"""
    while True:
        result_cb = await queue.get()
        
        if result_cb.error:
            raise result_cb.error
        
        # Check if this is stream data or final data
        if result_cb.has_stream_data:
            yield result_cb.stream_data
        elif result_cb.has_data:
            yield result_cb.data
            break  # Final data received
        else:
            break  # No more data

async def make_async_call(call_fn, *args) -> Any:
    """Make an async CFFI call and wait for result using queue-based approach"""
    # Generate unique ID
    call_id = random.randint(1, 1000000)
    
    # Create queue and register callback state
    loop = asyncio.get_event_loop()
    queue = asyncio.Queue()
    
    with _callback_lock:
        _active_callbacks[call_id] = CallbackState(
            queue=queue,
            loop=loop,
            type_map={}  # Will be populated in later phases
        )
    
    try:
        # Make the C call with our ID
        error = call_fn(*args, call_id)
        if error:
            # Synchronous error
            error_msg = error.decode('utf-8')
            raise RuntimeError(f"Call failed: {error_msg}")
        
        # Use wrapper to get single result from queue
        return await queue_to_single_result(queue)
    except Exception:
        # Cleanup on error
        with _callback_lock:
            _active_callbacks.pop(call_id, None)
        raise

async def make_async_call_with_type_map(call_fn, *args) -> Any:
    """Make an async CFFI call with a type map for encoding/decoding"""
    # Extract type_map from the last argument
    *call_args, type_map = args
    
    # Generate unique ID
    call_id = random.randint(1, 1000000)
    
    # Create queue and register callback state with type map
    loop = asyncio.get_event_loop()
    queue = asyncio.Queue()
    
    with _callback_lock:
        _active_callbacks[call_id] = CallbackState(
            queue=queue,
            loop=loop,
            type_map=type_map  # Store the type map for callback processing
        )
    
    try:
        # Make the C call with our ID
        error = call_fn(*call_args, call_id)
        if error:
            # Synchronous error
            error_msg = error.decode('utf-8')
            raise RuntimeError(f"Call failed: {error_msg}")
        
        # Use wrapper to get single result from queue
        return await queue_to_single_result(queue)
    except Exception:
        # Cleanup on error
        with _callback_lock:
            _active_callbacks.pop(call_id, None)
        raise