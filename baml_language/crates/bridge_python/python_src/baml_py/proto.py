"""Protobuf encoder/decoder for BAML bridge_ctypes protocol.

Uses generated protobuf classes from the .proto files in bridge_ctypes.
  - Encoding: Python kwargs → HostFunctionArguments protobuf bytes
  - Decoding: CFFIValueHolder protobuf bytes → Python values
"""

from __future__ import annotations

from typing import Any, Dict

from baml.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2


# ---------------------------------------------------------------------------
# Encoding: Python kwargs → HostFunctionArguments
# ---------------------------------------------------------------------------


def _set_host_value(host_value, value: Any) -> None:
    """Set a HostValue message from a Python value."""
    if value is None:
        # Leave oneof unset → null
        return
    if isinstance(value, bool):
        # Must check bool before int since bool is a subclass of int
        host_value.bool_value = value
    elif isinstance(value, int):
        host_value.int_value = value
    elif isinstance(value, float):
        host_value.float_value = value
    elif isinstance(value, str):
        host_value.string_value = value
    elif isinstance(value, (list, tuple)):
        list_val = host_value.list_value
        for item in value:
            _set_host_value(list_val.values.add(), item)
    elif isinstance(value, dict):
        map_val = host_value.map_value
        for k, v in value.items():
            _set_host_map_entry(map_val.entries.add(), k, v)
    else:
        raise TypeError(f"Cannot encode value of type {type(value).__name__} to protobuf")


def _set_host_map_entry(entry, key: Any, value: Any) -> None:
    """Set a HostMapEntry message from a key-value pair."""
    if isinstance(key, str):
        entry.string_key = key
    elif isinstance(key, bool):
        entry.bool_key = key
    elif isinstance(key, int):
        entry.int_key = key
    else:
        entry.string_key = str(key)
    _set_host_value(entry.value, value)


def encode_call_args(kwargs: Dict[str, Any]) -> bytes:
    """Encode function keyword arguments as HostFunctionArguments protobuf.

    Args:
        kwargs: dict mapping argument names to Python values

    Returns:
        Protobuf-encoded bytes for HostFunctionArguments
    """
    args = baml_inbound_pb2.HostFunctionArguments()
    for key, value in kwargs.items():
        _set_host_map_entry(args.kwargs.add(), key, value)
    return args.SerializeToString()


# ---------------------------------------------------------------------------
# Decoding: CFFIValueHolder → Python values
# ---------------------------------------------------------------------------


def _decode_value_holder(holder) -> Any:
    """Convert a CFFIValueHolder message to a Python value."""
    which = holder.WhichOneof("value")
    if which is None:
        return None
    if which == "null_value":
        return None
    if which == "string_value":
        return holder.string_value
    if which == "int_value":
        return holder.int_value
    if which == "float_value":
        return holder.float_value
    if which == "bool_value":
        return holder.bool_value
    if which == "class_value":
        return {
            entry.key: _decode_value_holder(entry.value)
            for entry in holder.class_value.fields
        }
    if which == "enum_value":
        return holder.enum_value.value
    if which == "literal_value":
        lit = holder.literal_value
        lit_which = lit.WhichOneof("literal")
        if lit_which == "string_literal":
            return lit.string_literal.value
        if lit_which == "int_literal":
            return lit.int_literal.value
        if lit_which == "bool_literal":
            return lit.bool_literal.value
        return None
    if which == "object_value":
        return None
    if which == "list_value":
        return [_decode_value_holder(item) for item in holder.list_value.items]
    if which == "map_value":
        return {
            entry.key: _decode_value_holder(entry.value)
            for entry in holder.map_value.entries
        }
    if which == "union_variant_value":
        return _decode_value_holder(holder.union_variant_value.value)
    if which == "checked_value":
        return _decode_value_holder(holder.checked_value.value)
    if which == "streaming_state_value":
        return _decode_value_holder(holder.streaming_state_value.value)
    return None


def decode_call_result(data: bytes) -> Any:
    """Decode a CFFIValueHolder protobuf to a Python value.

    Args:
        data: Protobuf-encoded CFFIValueHolder bytes

    Returns:
        Decoded Python value (None, bool, int, float, str, list, dict)
    """
    holder = baml_outbound_pb2.CFFIValueHolder()
    holder.ParseFromString(data)
    return _decode_value_holder(holder)
