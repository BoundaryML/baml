import ctypes
import threading
import asyncio
from typing import Dict, Optional, Callable, Any
from dataclasses import dataclass, field

from ._result_types import ResultCallback, BamlError

# Define callback function types
CALLBACK_FN = ctypes.CFUNCTYPE(None, ctypes.c_uint32, ctypes.c_int32, 
                               ctypes.POINTER(ctypes.c_int8), ctypes.c_size_t)
ON_TICK_CALLBACK_FN = ctypes.CFUNCTYPE(None, ctypes.c_uint32)

@dataclass
class CallbackState:
    """State for an active async call - uses queue-only approach"""
    queue: asyncio.Queue[ResultCallback]
    on_tick: Optional[Callable] = None
    loop: Optional[asyncio.AbstractEventLoop] = None
    type_map: Dict[str, Any] = field(default_factory=dict)

# Global callback registry (thread-safe)
_callback_lock = threading.RLock()
_active_callbacks: Dict[int, CallbackState] = {}

# Callback implementations
@CALLBACK_FN
def _trigger_callback(call_id: int, is_done: int, content_ptr, length: int):
    """Handle successful results using queue-based approach"""
    with _callback_lock:
        state = _active_callbacks.get(call_id)
        if not state:
            return
    
    # Create result callback
    result_cb = ResultCallback()
    
    # Decode callback data (will use serde module in later phases)
    if content_ptr and length > 0:
        try:
            data = ctypes.string_at(content_ptr, length)
            # TODO: Replace with actual serde decoder in Phase 4
            # decoded_value = decode_callback_data(data, state.type_map)
            decoded_value = data  # Temporary - just pass raw data
            
            if is_done:
                result_cb.has_data = True
                result_cb.data = decoded_value
            else:
                result_cb.has_stream_data = True
                result_cb.stream_data = decoded_value
        except Exception as e:
            result_cb.error = BamlError(f"Failed to decode callback data: {e}")
    
    # Always use queue - simpler logic
    if state.loop:
        state.loop.call_soon_threadsafe(
            lambda: state.queue.put_nowait(result_cb)
        )
    
    # Cleanup if done
    if is_done:
        with _callback_lock:
            _active_callbacks.pop(call_id, None)

@CALLBACK_FN  
def _error_callback(call_id: int, is_done: int, content_ptr, length: int):
    """Handle errors using queue-based approach"""
    with _callback_lock:
        state = _active_callbacks.get(call_id)
        if not state:
            return
    
    # Extract error message and create error callback
    error_msg = ctypes.string_at(content_ptr, length).decode('utf-8')
    error = BamlError(error_msg)
    
    result_cb = ResultCallback(error=error)
    
    # Send error through queue
    if state.loop:
        state.loop.call_soon_threadsafe(
            lambda: state.queue.put_nowait(result_cb)
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