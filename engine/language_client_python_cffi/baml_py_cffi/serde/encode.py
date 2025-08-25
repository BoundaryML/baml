import os
from typing import Any, List, Dict, Optional
from . import cffi_pb2


def encode_list(values: List[Any]) -> cffi_pb2.CFFIValueList:
    """Encode a list of values."""
    list_value = cffi_pb2.CFFIValueList()

    for item in values:
        list_value.values.append(encode_value(item))

    return list_value


def encode_map(values: Dict[str, Any]) -> cffi_pb2.CFFIValueMap:
    """Encode a map with string keys and any values."""
    map_value = cffi_pb2.CFFIValueMap()

    for key, val in values.items():
        entry = cffi_pb2.CFFIMapEntry()
        entry.key = str(key)  # Ensure key is string
        entry.value.CopyFrom(encode_value(val))
        map_value.entries.append(entry)

    return map_value


def encode_value(value: Any) -> cffi_pb2.CFFIValueHolder:
    """Encode a value to CFFI format - primitives, lists, and maps."""
    holder = cffi_pb2.CFFIValueHolder()

    if value is None:
        holder.null_value.CopyFrom(cffi_pb2.CFFIValueNull())
    elif isinstance(value, bool):
        # Must check bool before int as bool is subclass of int
        holder.bool_value = value
    elif isinstance(value, int):
        holder.int_value = value
    elif isinstance(value, float):
        holder.float_value = value
    elif isinstance(value, str):
        holder.string_value = value
    elif isinstance(value, list):
        holder.list_value.CopyFrom(encode_list(value))
    elif isinstance(value, dict):
        holder.map_value.CopyFrom(encode_map(value))
    else:
        raise ValueError(f"Unsupported type: {type(value)}")

    return holder


def encode_function_args(
    args: Dict[str, Any], 
    env_vars: Optional[Dict[str, str]] = None
) -> bytes:
    """
    Encode function arguments for BAML runtime calls.
    
    Args:
        args: Dictionary of function arguments to encode
        env_vars: Optional dictionary of environment variables. 
                 If None, uses os.environ
    
    Returns:
        Serialized CFFIFunctionArguments protobuf
    """
    func_args = cffi_pb2.CFFIFunctionArguments()
    
    # Add environment variables
    env_to_use = env_vars if env_vars is not None else os.environ
    for env_var, env_value in env_to_use.items():
        entry = cffi_pb2.CFFIEnvVar()
        entry.key = env_var
        entry.value = env_value
        func_args.env.append(entry)
    
    # Encode each argument
    for key, value in args.items():
        holder = encode_value(value)
        entry = cffi_pb2.CFFIMapEntry()
        entry.key = key
        entry.value.CopyFrom(holder)
        func_args.kwargs.append(entry)
    
    return func_args.SerializeToString()
