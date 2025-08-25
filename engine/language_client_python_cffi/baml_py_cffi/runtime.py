import ctypes
import json
from typing import Dict, Any, Optional
from ._ffi import _lib
from ._callbacks import register_callbacks
from ._async_utils import make_async_call
from .serde.type_map import TypeMap
from .serde.encode import encode_value, encode_function_args
from .serde.decode import decode_value
from .serde import cffi_pb2


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
    
    async def call_function_typed(
        self,
        name: str,
        args: Dict[str, Any],
        type_map: TypeMap,
        arg_types: Dict[str, str],
        return_type: str,
        env_vars: Optional[Dict[str, str]] = None
    ) -> Any:
        """Call a BAML function with a scoped type map."""
        # Create function arguments protobuf
        func_args = cffi_pb2.CFFIFunctionArguments()
        # Note: function_name is passed separately, not as part of CFFIFunctionArguments
        
        # Add environment variables
        import os
        env_to_use = env_vars if env_vars is not None else os.environ
        for env_var, env_value in env_to_use.items():
            entry = cffi_pb2.CFFIEnvVar()
            entry.key = env_var
            entry.value = env_value
            func_args.env.append(entry)
        
        # Encode arguments with type map
        for arg_name, arg_value in args.items():
            arg_type = arg_types.get(arg_name)
            holder = encode_value(arg_value, arg_type, type_map)
            entry = cffi_pb2.CFFIMapEntry()
            entry.key = arg_name
            entry.value.CopyFrom(holder)
            func_args.kwargs.append(entry)
        
        # Serialize to bytes
        encoded_args = func_args.SerializeToString()
        
        # Make the call with type map attached to callback state
        from ._async_utils import make_async_call_with_type_map
        result_bytes = await make_async_call_with_type_map(
            _lib.call_function_from_c,
            self._ptr,
            name.encode("utf-8"),
            encoded_args,
            len(encoded_args),
            type_map
        )
        
        # Decode result with type map
        holder = cffi_pb2.CFFIValueHolder()
        holder.ParseFromString(result_bytes)
        return decode_value(holder, return_type, type_map)
