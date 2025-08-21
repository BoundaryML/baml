import ctypes
import json
from typing import Dict, Any, Optional
from ._ffi import _lib
from ._callbacks import register_callbacks
from ._async_utils import make_async_call


class BamlRuntime:
    """BAML Runtime for executing functions"""

    def __init__(self, root_path: str, src_files: Dict[str, str], env_vars: Dict[str, str]):
        """Create a new BAML runtime"""
        # Register callbacks on first runtime creation
        if not hasattr(_lib, "_callbacks_registered"):
            register_callbacks(_lib)
            _lib._callbacks_registered = True

        # Define create_baml_runtime signature
        _lib.create_baml_runtime.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p]
        _lib.create_baml_runtime.restype = ctypes.c_void_p

        # Create runtime
        self._ptr = _lib.create_baml_runtime(
            root_path.encode("utf-8"),
            json.dumps(src_files).encode("utf-8"),
            json.dumps(env_vars).encode("utf-8"),
        )

        if not self._ptr:
            raise RuntimeError("Failed to create BAML runtime")

    def __del__(self):
        """Clean up runtime"""
        if hasattr(self, "_ptr") and self._ptr:
            _lib.destroy_baml_runtime.argtypes = [ctypes.c_void_p]
            _lib.destroy_baml_runtime(self._ptr)

    async def call_function(self, function_name: str, args: bytes) -> bytes:
        """Call a BAML function asynchronously"""
        # Define function signature
        _lib.call_function_from_c.argtypes = [
            ctypes.c_void_p,  # runtime
            ctypes.c_char_p,  # function_name
            ctypes.c_char_p,  # encoded_args
            ctypes.c_size_t,  # length
            ctypes.c_uint32,  # call_id
        ]
        _lib.call_function_from_c.restype = ctypes.c_char_p

        # Make async call
        return await make_async_call(
            _lib.call_function_from_c, self._ptr, function_name.encode("utf-8"), args, len(args)
        )
