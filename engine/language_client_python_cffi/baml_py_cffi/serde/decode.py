from typing import Any, List, Dict, Optional
from . import cffi_pb2
from .type_map import TypeMap

def decode_list(holder: cffi_pb2.CFFIValueHolder, type_map: Optional[TypeMap] = None) -> List[Any]:
    """Decode a list of values."""
    return [decode_value(item, type_map=type_map) for item in holder.list_value.values]

def decode_map(holder: cffi_pb2.CFFIValueHolder, type_map: Optional[TypeMap] = None) -> Dict[str, Any]:
    """Decode a map with string keys."""
    result = {}
    for entry in holder.map_value.entries:
        result[entry.key] = decode_value(entry.value, type_map=type_map)
    return result

def decode_dynamic_class(holder: cffi_pb2.CFFIValueHolder, type_map: Optional[TypeMap] = None) -> 'DynamicClass':
    """Create dynamic class when not in type map (like Go)."""
    class DynamicClass:
        def __init__(self):
            self._name = holder.class_value.name.name if holder.class_value.name else "UnknownClass"
            self._fields = {}
            for field in holder.class_value.fields:
                self._fields[field.key] = decode_value(field.value, type_map=type_map)
        
        def __repr__(self):
            return f"DynamicClass({self._name}, {self._fields})"
        
        def __getattr__(self, name):
            if name in self._fields:
                return self._fields[name]
            raise AttributeError(f"'{self.__class__.__name__}' object has no attribute '{name}'")
    
    return DynamicClass()

def decode_dynamic_enum(holder: cffi_pb2.CFFIValueHolder) -> 'DynamicEnum':
    """Create dynamic enum when not in type map (like Go)."""
    class DynamicEnum:
        def __init__(self):
            self._type = holder.enum_value.name.name if holder.enum_value.name else "UnknownEnum"
            self._value = holder.enum_value.value
        
        def __repr__(self):
            return f"DynamicEnum({self._type}::{self._value})"
        
        def __str__(self):
            return self._value
    
    return DynamicEnum()

def decode_value(holder: cffi_pb2.CFFIValueHolder, type_name: Optional[str] = None, 
                type_map: Optional[TypeMap] = None) -> Any:
    """Decode a value from CFFI format with optional type map."""
    # Check type map first if provided
    if type_name and type_map and type_name in type_map:
        _, _, decoder = type_map[type_name]
        return decoder(holder)
    
    # Handle all value types
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
        return decode_list(holder, type_map)
    elif holder.HasField('map_value'):
        return decode_map(holder, type_map)
    elif holder.HasField('class_value'):
        # Try to decode using type map if available
        if type_map and holder.class_value.name:
            class_name = holder.class_value.name.name
            if class_name in type_map:
                _, _, decoder = type_map[class_name]
                return decoder(holder)
        # Fall back to dynamic class
        return decode_dynamic_class(holder, type_map)
    elif holder.HasField('enum_value'):
        # Try to decode using type map if available
        if type_map and holder.enum_value.name:
            enum_name = holder.enum_value.name.name
            if enum_name in type_map:
                _, _, decoder = type_map[enum_name]
                return decoder(holder)
        # Fall back to dynamic enum
        return decode_dynamic_enum(holder)
    else:
        raise ValueError("Unknown type in holder")