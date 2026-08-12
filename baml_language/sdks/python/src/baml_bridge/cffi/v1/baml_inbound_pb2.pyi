from baml_bridge.cffi.v1 import baml_handle_pb2 as _baml_handle_pb2
from baml_bridge.cffi.v1 import baml_type_pb2 as _baml_type_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class InboundValue(_message.Message):
    __slots__ = ("value_type", "string_value", "int_value", "float_value", "bool_value", "list_value", "map_value", "class_value", "enum_value", "handle", "uint8array_value", "bigint_value", "ty_value")
    VALUE_TYPE_FIELD_NUMBER: _ClassVar[int]
    STRING_VALUE_FIELD_NUMBER: _ClassVar[int]
    INT_VALUE_FIELD_NUMBER: _ClassVar[int]
    FLOAT_VALUE_FIELD_NUMBER: _ClassVar[int]
    BOOL_VALUE_FIELD_NUMBER: _ClassVar[int]
    LIST_VALUE_FIELD_NUMBER: _ClassVar[int]
    MAP_VALUE_FIELD_NUMBER: _ClassVar[int]
    CLASS_VALUE_FIELD_NUMBER: _ClassVar[int]
    ENUM_VALUE_FIELD_NUMBER: _ClassVar[int]
    HANDLE_FIELD_NUMBER: _ClassVar[int]
    UINT8ARRAY_VALUE_FIELD_NUMBER: _ClassVar[int]
    BIGINT_VALUE_FIELD_NUMBER: _ClassVar[int]
    TY_VALUE_FIELD_NUMBER: _ClassVar[int]
    value_type: _baml_type_pb2.BamlTy
    string_value: str
    int_value: int
    float_value: float
    bool_value: bool
    list_value: InboundListValue
    map_value: InboundMapValue
    class_value: InboundClassValue
    enum_value: InboundEnumValue
    handle: _baml_handle_pb2.BamlHandle
    uint8array_value: bytes
    bigint_value: str
    ty_value: _baml_type_pb2.BamlTy
    def __init__(self, value_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., list_value: _Optional[_Union[InboundListValue, _Mapping]] = ..., map_value: _Optional[_Union[InboundMapValue, _Mapping]] = ..., class_value: _Optional[_Union[InboundClassValue, _Mapping]] = ..., enum_value: _Optional[_Union[InboundEnumValue, _Mapping]] = ..., handle: _Optional[_Union[_baml_handle_pb2.BamlHandle, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ..., bigint_value: _Optional[str] = ..., ty_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ...) -> None: ...

class InboundListValue(_message.Message):
    __slots__ = ("values",)
    VALUES_FIELD_NUMBER: _ClassVar[int]
    values: _containers.RepeatedCompositeFieldContainer[InboundValue]
    def __init__(self, values: _Optional[_Iterable[_Union[InboundValue, _Mapping]]] = ...) -> None: ...

class InboundMapValue(_message.Message):
    __slots__ = ("entries",)
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    def __init__(self, entries: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ...) -> None: ...

class InboundMapEntry(_message.Message):
    __slots__ = ("string_key", "int_key", "bool_key", "enum_key", "value")
    STRING_KEY_FIELD_NUMBER: _ClassVar[int]
    INT_KEY_FIELD_NUMBER: _ClassVar[int]
    BOOL_KEY_FIELD_NUMBER: _ClassVar[int]
    ENUM_KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    string_key: str
    int_key: int
    bool_key: bool
    enum_key: InboundEnumValue
    value: InboundValue
    def __init__(self, string_key: _Optional[str] = ..., int_key: _Optional[int] = ..., bool_key: bool = ..., enum_key: _Optional[_Union[InboundEnumValue, _Mapping]] = ..., value: _Optional[_Union[InboundValue, _Mapping]] = ...) -> None: ...

class InboundClassValue(_message.Message):
    __slots__ = ("fields",)
    FIELDS_FIELD_NUMBER: _ClassVar[int]
    fields: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    def __init__(self, fields: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ...) -> None: ...

class InboundEnumValue(_message.Message):
    __slots__ = ("name", "value")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    name: str
    value: str
    def __init__(self, name: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...

class BamlTyArg(_message.Message):
    __slots__ = ("type_var", "type_value")
    TYPE_VAR_FIELD_NUMBER: _ClassVar[int]
    TYPE_VALUE_FIELD_NUMBER: _ClassVar[int]
    type_var: str
    type_value: _baml_type_pb2.BamlTy
    def __init__(self, type_var: _Optional[str] = ..., type_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ...) -> None: ...

class CallFunctionArgs(_message.Message):
    __slots__ = ("kwargs", "call_id", "type_args", "function_name", "function_handle")
    KWARGS_FIELD_NUMBER: _ClassVar[int]
    CALL_ID_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_NAME_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_HANDLE_FIELD_NUMBER: _ClassVar[int]
    kwargs: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    call_id: int
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    function_name: str
    function_handle: int
    def __init__(self, kwargs: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ..., call_id: _Optional[int] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ..., function_name: _Optional[str] = ..., function_handle: _Optional[int] = ...) -> None: ...

class CallAck(_message.Message):
    __slots__ = ("error",)
    ERROR_FIELD_NUMBER: _ClassVar[int]
    error: str
    def __init__(self, error: _Optional[str] = ...) -> None: ...
