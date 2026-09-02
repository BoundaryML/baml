from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
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
    HOST_VALUE_CALLABLE: _ClassVar[BamlHandleType]
    HOST_VALUE_OPAQUE: _ClassVar[BamlHandleType]
    ADT_FUNCTION_SPEC: _ClassVar[BamlHandleType]
    ADT_RUNTIME_VALUE: _ClassVar[BamlHandleType]
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
HOST_VALUE_CALLABLE: BamlHandleType
HOST_VALUE_OPAQUE: BamlHandleType
ADT_FUNCTION_SPEC: BamlHandleType
ADT_RUNTIME_VALUE: BamlHandleType

class BamlHandle(_message.Message):
    __slots__ = ("key", "handle_type")
    KEY_FIELD_NUMBER: _ClassVar[int]
    HANDLE_TYPE_FIELD_NUMBER: _ClassVar[int]
    key: int
    handle_type: BamlHandleType
    def __init__(self, key: _Optional[int] = ..., handle_type: _Optional[_Union[BamlHandleType, str]] = ...) -> None: ...
