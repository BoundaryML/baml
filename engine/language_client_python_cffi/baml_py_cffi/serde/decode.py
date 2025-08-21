from typing import Any, List, Dict
from . import cffi_pb2

def decode_list(holder: cffi_pb2.CFFIValueHolder) -> List[Any]:
    """Decode a list of values."""
    return [decode_value(item) for item in holder.list_value.values]

def decode_map(holder: cffi_pb2.CFFIValueHolder) -> Dict[str, Any]:
    """Decode a map with string keys."""
    result = {}
    for entry in holder.map_value.entries:
        result[entry.key] = decode_value(entry.value)
    return result

def decode_value(holder: cffi_pb2.CFFIValueHolder) -> Any:
    """Decode a value from CFFI format - primitives, lists, and maps."""
    if holder.HasField('null_value'):
        return None
    elif holder.HasField('bool_value'):
        return holder.bool_value
    elif holder.HasField('int_value'):
        return holder.int_value
    elif holder.HasField('float_value'):
        return holder.float_value
    elif holder.HasField('string_value'):
        return holder.string_value
    elif holder.HasField('list_value'):
        return decode_list(holder)
    elif holder.HasField('map_value'):
        return decode_map(holder)
    else:
        raise ValueError("Unknown type in holder")