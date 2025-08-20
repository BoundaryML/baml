import asyncio
import random
from typing import Optional, Any
from ._callbacks import CallbackState, _active_callbacks, _callback_lock

async def make_async_call(call_fn, *args) -> Any:
    """Make an async CFFI call and wait for result"""
    # Generate unique ID
    call_id = random.randint(1, 1000000)
    
    # Create future and register callback state
    loop = asyncio.get_event_loop()
    future = loop.create_future()
    
    with _callback_lock:
        _active_callbacks[call_id] = CallbackState(
            future=future,
            loop=loop
        )
    
    try:
        # Make the C call with our ID
        error = call_fn(*args, call_id)
        if error:
            # Synchronous error
            error_msg = error.decode('utf-8')
            future.set_exception(RuntimeError(f"Call failed: {error_msg}"))
        
        # Wait for async result
        return await future
    except Exception:
        # Cleanup on error
        with _callback_lock:
            _active_callbacks.pop(call_id, None)
        raise