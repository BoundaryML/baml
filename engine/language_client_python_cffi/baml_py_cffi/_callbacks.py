import ctypes
import threading
import asyncio
from typing import Dict, Optional, Callable
from dataclasses import dataclass

# Define callback function types
CALLBACK_FN = ctypes.CFUNCTYPE(None, ctypes.c_uint32, ctypes.c_int32, 
                               ctypes.POINTER(ctypes.c_int8), ctypes.c_size_t)
ON_TICK_CALLBACK_FN = ctypes.CFUNCTYPE(None, ctypes.c_uint32)

@dataclass
class CallbackState:
    """State for an active async call"""
    future: asyncio.Future
    queue: Optional[asyncio.Queue] = None
    on_tick: Optional[Callable] = None
    loop: Optional[asyncio.AbstractEventLoop] = None

# Global callback registry (thread-safe)
_callback_lock = threading.RLock()
_active_callbacks: Dict[int, CallbackState] = {}

# Callback implementations
@CALLBACK_FN
def _trigger_callback(call_id: int, is_done: int, content_ptr, length: int):
    """Handle successful results"""
    with _callback_lock:
        state = _active_callbacks.get(call_id)
        if not state:
            return
    
    # Copy data from C pointer
    if content_ptr and length > 0:
        data = ctypes.string_at(content_ptr, length)
        # TODO: Parse protobuf in next phase
        result = data
    else:
        result = None
    
    # Handle streaming vs single result
    if state.queue and state.loop is not None:  # Streaming
        # Always send the result if there is one
        if result is not None:
            state.loop.call_soon_threadsafe(
                lambda: state.queue.put_nowait(result)
            )
        # Send sentinel after the last result
        if is_done:
            state.loop.call_soon_threadsafe(
                lambda: state.queue.put_nowait(None)
            )
    else:  # Single result
        if state.loop and not state.future.done():
            state.loop.call_soon_threadsafe(
                lambda: state.future.set_result(result)
            )
    
    # Cleanup if done
    if is_done:
        with _callback_lock:
            _active_callbacks.pop(call_id, None)

@CALLBACK_FN  
def _error_callback(call_id: int, is_done: int, content_ptr, length: int):
    """Handle errors"""
    with _callback_lock:
        state = _active_callbacks.get(call_id)
        if not state:
            return
    
    # Extract error message
    error_msg = ctypes.string_at(content_ptr, length).decode('utf-8')
    error = RuntimeError(f"BAML Error: {error_msg}")
    
    # Schedule error on the event loop
    if state.loop:
        state.loop.call_soon_threadsafe(
            lambda: state.future.set_exception(error) if not state.future.done() else None
        )
    
    # Cleanup
    with _callback_lock:
        _active_callbacks.pop(call_id, None)

@ON_TICK_CALLBACK_FN
def _tick_callback(call_id: int):
    """Handle progress notifications"""
    with _callback_lock:
        state = _active_callbacks.get(call_id)
        if state and state.on_tick:
            # TODO: Implement tick handling
            pass

def register_callbacks(lib: ctypes.CDLL):
    """Register callbacks with the CFFI library"""
    lib.register_callbacks.argtypes = [CALLBACK_FN, CALLBACK_FN, ON_TICK_CALLBACK_FN]
    lib.register_callbacks(_trigger_callback, _error_callback, _tick_callback)