from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class BamlHandleType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    HANDLE_UNSPECIFIED: _ClassVar[BamlHandleType]
    UNTAGGED_RUST_DATA: _ClassVar[BamlHandleType]
    UNTAGGED_BEX_HEAP: _ClassVar[BamlHandleType]
    FUNCTION_REF: _ClassVar[BamlHandleType]
    ADT_MEDIA_IMAGE: _ClassVar[BamlHandleType]
    ADT_MEDIA_AUDIO: _ClassVar[BamlHandleType]
    ADT_MEDIA_VIDEO: _ClassVar[BamlHandleType]
    ADT_MEDIA_PDF: _ClassVar[BamlHandleType]
    ADT_MEDIA_GENERIC: _ClassVar[BamlHandleType]
    ADT_PROMPT_AST: _ClassVar[BamlHandleType]
    ADT_COLLECTOR: _ClassVar[BamlHandleType]
    ADT_TYPE: _ClassVar[BamlHandleType]
    ADT_TAGGED_HEAP_HANDLE: _ClassVar[BamlHandleType]
HANDLE_UNSPECIFIED: BamlHandleType
UNTAGGED_RUST_DATA: BamlHandleType
UNTAGGED_BEX_HEAP: BamlHandleType
FUNCTION_REF: BamlHandleType
ADT_MEDIA_IMAGE: BamlHandleType
ADT_MEDIA_AUDIO: BamlHandleType
ADT_MEDIA_VIDEO: BamlHandleType
ADT_MEDIA_PDF: BamlHandleType
ADT_MEDIA_GENERIC: BamlHandleType
ADT_PROMPT_AST: BamlHandleType
ADT_COLLECTOR: BamlHandleType
ADT_TYPE: BamlHandleType
ADT_TAGGED_HEAP_HANDLE: BamlHandleType

class BamlHandle(_message.Message):
    __slots__ = ("key", "handle_type")
    KEY_FIELD_NUMBER: _ClassVar[int]
    HANDLE_TYPE_FIELD_NUMBER: _ClassVar[int]
    key: int
    handle_type: BamlHandleType
    def __init__(self, key: _Optional[int] = ..., handle_type: _Optional[_Union[BamlHandleType, str]] = ...) -> None: ...

class InboundValue(_message.Message):
    __slots__ = ("string_value", "int_value", "float_value", "bool_value", "list_value", "map_value", "class_value", "enum_value", "handle", "uint8array_value")
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
    string_value: str
    int_value: int
    float_value: float
    bool_value: bool
    list_value: InboundListValue
    map_value: InboundMapValue
    class_value: InboundClassValue
    enum_value: InboundEnumValue
    handle: BamlHandle
    uint8array_value: bytes
    def __init__(self, string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., list_value: _Optional[_Union[InboundListValue, _Mapping]] = ..., map_value: _Optional[_Union[InboundMapValue, _Mapping]] = ..., class_value: _Optional[_Union[InboundClassValue, _Mapping]] = ..., enum_value: _Optional[_Union[InboundEnumValue, _Mapping]] = ..., handle: _Optional[_Union[BamlHandle, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ...) -> None: ...

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
    __slots__ = ("name", "fields")
    NAME_FIELD_NUMBER: _ClassVar[int]
    FIELDS_FIELD_NUMBER: _ClassVar[int]
    name: str
    fields: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    def __init__(self, name: _Optional[str] = ..., fields: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ...) -> None: ...

class InboundEnumValue(_message.Message):
    __slots__ = ("name", "value")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    name: str
    value: str
    def __init__(self, name: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...

class CallFunctionArgs(_message.Message):
    __slots__ = ("kwargs",)
    KWARGS_FIELD_NUMBER: _ClassVar[int]
    kwargs: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    def __init__(self, kwargs: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ...) -> None: ...

class CallAck(_message.Message):
    __slots__ = ("error",)
    ERROR_FIELD_NUMBER: _ClassVar[int]
    error: str
    def __init__(self, error: _Optional[str] = ...) -> None: ...
