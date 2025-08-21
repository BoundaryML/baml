from typing import Any, List, Dict
from . import cffi_pb2


def encode_list(values: List[Any]) -> cffi_pb2.CFFIValueHolder:
    """Encode a list of values."""
    holder = cffi_pb2.CFFIValueHolder()
    list_value = cffi_pb2.CFFIValueList()

    for item in values:
        list_value.values.append(encode_value(item))

    holder.list_value.CopyFrom(list_value)
    return holder


def encode_map(values: Dict[str, Any]) -> cffi_pb2.CFFIValueHolder:
    """Encode a map with string keys and any values."""
    holder = cffi_pb2.CFFIValueHolder()
    map_value = cffi_pb2.CFFIValueMap()

    for key, val in values.items():
        entry = cffi_pb2.CFFIMapEntry()
        entry.key = str(key)  # Ensure key is string
        entry.value.CopyFrom(encode_value(val))
        map_value.entries.append(entry)

    holder.map_value.CopyFrom(map_value)
    return holder


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
        return encode_list(value)
    elif isinstance(value, dict):
        return encode_map(value)
    else:
        raise ValueError(f"Unsupported type: {type(value)}")

    return holder
