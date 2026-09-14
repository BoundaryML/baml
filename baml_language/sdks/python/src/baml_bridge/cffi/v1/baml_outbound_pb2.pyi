from baml_bridge.cffi.v1 import baml_handle_pb2 as _baml_handle_pb2
from baml_bridge.cffi.v1 import baml_type_pb2 as _baml_type_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class MediaTypeEnum(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    MEDIA_TYPE_UNSPECIFIED: _ClassVar[MediaTypeEnum]
    IMAGE: _ClassVar[MediaTypeEnum]
    AUDIO: _ClassVar[MediaTypeEnum]
    PDF: _ClassVar[MediaTypeEnum]
    VIDEO: _ClassVar[MediaTypeEnum]
    OTHER: _ClassVar[MediaTypeEnum]
MEDIA_TYPE_UNSPECIFIED: MediaTypeEnum
IMAGE: MediaTypeEnum
AUDIO: MediaTypeEnum
PDF: MediaTypeEnum
VIDEO: MediaTypeEnum
OTHER: MediaTypeEnum

class BamlOutboundResult(_message.Message):
    __slots__ = ("ok", "error", "panic")
    OK_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    PANIC_FIELD_NUMBER: _ClassVar[int]
    ok: BamlOutboundValue
    error: BamlOutboundError
    panic: BamlOutboundPanic
    def __init__(self, ok: _Optional[_Union[BamlOutboundValue, _Mapping]] = ..., error: _Optional[_Union[BamlOutboundError, _Mapping]] = ..., panic: _Optional[_Union[BamlOutboundPanic, _Mapping]] = ...) -> None: ...

class BamlOutboundError(_message.Message):
    __slots__ = ("value", "trace")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    TRACE_FIELD_NUMBER: _ClassVar[int]
    value: BamlOutboundValue
    trace: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ..., trace: _Optional[_Iterable[str]] = ...) -> None: ...

class BamlOutboundPanic(_message.Message):
    __slots__ = ("value", "trace", "is_exit_panic", "exit_code")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    TRACE_FIELD_NUMBER: _ClassVar[int]
    IS_EXIT_PANIC_FIELD_NUMBER: _ClassVar[int]
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    value: BamlOutboundValue
    trace: _containers.RepeatedScalarFieldContainer[str]
    is_exit_panic: bool
    exit_code: int
    def __init__(self, value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ..., trace: _Optional[_Iterable[str]] = ..., is_exit_panic: bool = ..., exit_code: _Optional[int] = ...) -> None: ...

class BamlOutboundValue(_message.Message):
    __slots__ = ("null_value", "string_value", "int_value", "float_value", "bool_value", "class_value", "enum_value", "literal_value", "list_value", "map_value", "union_variant_value", "handle_value", "media_value", "prompt_ast_value", "uint8array_value", "bigint_value", "ty_value", "ty_def_value")
    NULL_VALUE_FIELD_NUMBER: _ClassVar[int]
    STRING_VALUE_FIELD_NUMBER: _ClassVar[int]
    INT_VALUE_FIELD_NUMBER: _ClassVar[int]
    FLOAT_VALUE_FIELD_NUMBER: _ClassVar[int]
    BOOL_VALUE_FIELD_NUMBER: _ClassVar[int]
    CLASS_VALUE_FIELD_NUMBER: _ClassVar[int]
    ENUM_VALUE_FIELD_NUMBER: _ClassVar[int]
    LITERAL_VALUE_FIELD_NUMBER: _ClassVar[int]
    LIST_VALUE_FIELD_NUMBER: _ClassVar[int]
    MAP_VALUE_FIELD_NUMBER: _ClassVar[int]
    UNION_VARIANT_VALUE_FIELD_NUMBER: _ClassVar[int]
    HANDLE_VALUE_FIELD_NUMBER: _ClassVar[int]
    MEDIA_VALUE_FIELD_NUMBER: _ClassVar[int]
    PROMPT_AST_VALUE_FIELD_NUMBER: _ClassVar[int]
    UINT8ARRAY_VALUE_FIELD_NUMBER: _ClassVar[int]
    BIGINT_VALUE_FIELD_NUMBER: _ClassVar[int]
    TY_VALUE_FIELD_NUMBER: _ClassVar[int]
    TY_DEF_VALUE_FIELD_NUMBER: _ClassVar[int]
    null_value: BamlValueNull
    string_value: str
    int_value: int
    float_value: float
    bool_value: bool
    class_value: BamlValueClass
    enum_value: BamlValueEnum
    literal_value: BamlLiteralValue
    list_value: BamlValueList
    map_value: BamlValueMap
    union_variant_value: BamlValueUnionVariant
    handle_value: BamlOutboundHandle
    media_value: BamlValueMedia
    prompt_ast_value: BamlValuePromptAst
    uint8array_value: bytes
    bigint_value: str
    ty_value: _baml_type_pb2.BamlTy
    ty_def_value: _baml_type_pb2.BamlTyDef
    def __init__(self, null_value: _Optional[_Union[BamlValueNull, _Mapping]] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., class_value: _Optional[_Union[BamlValueClass, _Mapping]] = ..., enum_value: _Optional[_Union[BamlValueEnum, _Mapping]] = ..., literal_value: _Optional[_Union[BamlLiteralValue, _Mapping]] = ..., list_value: _Optional[_Union[BamlValueList, _Mapping]] = ..., map_value: _Optional[_Union[BamlValueMap, _Mapping]] = ..., union_variant_value: _Optional[_Union[BamlValueUnionVariant, _Mapping]] = ..., handle_value: _Optional[_Union[BamlOutboundHandle, _Mapping]] = ..., media_value: _Optional[_Union[BamlValueMedia, _Mapping]] = ..., prompt_ast_value: _Optional[_Union[BamlValuePromptAst, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ..., bigint_value: _Optional[str] = ..., ty_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., ty_def_value: _Optional[_Union[_baml_type_pb2.BamlTyDef, _Mapping]] = ...) -> None: ...

class BamlOutboundHandle(_message.Message):
    __slots__ = ("key", "handle_type", "ty")
    KEY_FIELD_NUMBER: _ClassVar[int]
    HANDLE_TYPE_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    key: int
    handle_type: _baml_handle_pb2.BamlHandleType
    ty: _baml_type_pb2.BamlTy
    def __init__(self, key: _Optional[int] = ..., handle_type: _Optional[_Union[_baml_handle_pb2.BamlHandleType, str]] = ..., ty: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ...) -> None: ...

class BamlValueNull(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlValueList(_message.Message):
    __slots__ = ("item_type", "items")
    ITEM_TYPE_FIELD_NUMBER: _ClassVar[int]
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    item_type: _baml_type_pb2.BamlTy
    items: _containers.RepeatedCompositeFieldContainer[BamlOutboundValue]
    def __init__(self, item_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., items: _Optional[_Iterable[_Union[BamlOutboundValue, _Mapping]]] = ...) -> None: ...

class BamlOutboundMapEntry(_message.Message):
    __slots__ = ("key", "value")
    KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    key: str
    value: BamlOutboundValue
    def __init__(self, key: _Optional[str] = ..., value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ...) -> None: ...

class BamlValueMap(_message.Message):
    __slots__ = ("key_type", "value_type", "entries")
    KEY_TYPE_FIELD_NUMBER: _ClassVar[int]
    VALUE_TYPE_FIELD_NUMBER: _ClassVar[int]
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    key_type: _baml_type_pb2.BamlTy
    value_type: _baml_type_pb2.BamlTy
    entries: _containers.RepeatedCompositeFieldContainer[BamlOutboundMapEntry]
    def __init__(self, key_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., value_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., entries: _Optional[_Iterable[_Union[BamlOutboundMapEntry, _Mapping]]] = ...) -> None: ...

class BamlValueClass(_message.Message):
    __slots__ = ("name", "fields", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    FIELDS_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    fields: _containers.RepeatedCompositeFieldContainer[BamlOutboundMapEntry]
    type_args: _containers.RepeatedCompositeFieldContainer[_baml_type_pb2.BamlTy]
    def __init__(self, name: _Optional[str] = ..., fields: _Optional[_Iterable[_Union[BamlOutboundMapEntry, _Mapping]]] = ..., type_args: _Optional[_Iterable[_Union[_baml_type_pb2.BamlTy, _Mapping]]] = ...) -> None: ...

class BamlValueEnum(_message.Message):
    __slots__ = ("name", "value", "is_dynamic")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    IS_DYNAMIC_FIELD_NUMBER: _ClassVar[int]
    name: str
    value: str
    is_dynamic: bool
    def __init__(self, name: _Optional[str] = ..., value: _Optional[str] = ..., is_dynamic: bool = ...) -> None: ...

class BamlValueUnionVariant(_message.Message):
    __slots__ = ("name", "is_optional", "is_single_pattern", "self_type", "value_option_name", "value", "selected_option_index")
    NAME_FIELD_NUMBER: _ClassVar[int]
    IS_OPTIONAL_FIELD_NUMBER: _ClassVar[int]
    IS_SINGLE_PATTERN_FIELD_NUMBER: _ClassVar[int]
    SELF_TYPE_FIELD_NUMBER: _ClassVar[int]
    VALUE_OPTION_NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    SELECTED_OPTION_INDEX_FIELD_NUMBER: _ClassVar[int]
    name: str
    is_optional: bool
    is_single_pattern: bool
    self_type: _baml_type_pb2.BamlTy
    value_option_name: str
    value: BamlOutboundValue
    selected_option_index: int
    def __init__(self, name: _Optional[str] = ..., is_optional: bool = ..., is_single_pattern: bool = ..., self_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., value_option_name: _Optional[str] = ..., value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ..., selected_option_index: _Optional[int] = ...) -> None: ...

class BamlValueMedia(_message.Message):
    __slots__ = ("media", "mime_type", "url", "base64", "file")
    MEDIA_FIELD_NUMBER: _ClassVar[int]
    MIME_TYPE_FIELD_NUMBER: _ClassVar[int]
    URL_FIELD_NUMBER: _ClassVar[int]
    BASE64_FIELD_NUMBER: _ClassVar[int]
    FILE_FIELD_NUMBER: _ClassVar[int]
    media: MediaTypeEnum
    mime_type: str
    url: str
    base64: str
    file: str
    def __init__(self, media: _Optional[_Union[MediaTypeEnum, str]] = ..., mime_type: _Optional[str] = ..., url: _Optional[str] = ..., base64: _Optional[str] = ..., file: _Optional[str] = ...) -> None: ...

class BamlValuePromptAst(_message.Message):
    __slots__ = ("simple", "message", "multiple")
    SIMPLE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    MULTIPLE_FIELD_NUMBER: _ClassVar[int]
    simple: BamlValuePromptAstSimple
    message: BamlValuePromptAstMessage
    multiple: BamlValuePromptAstMultiple
    def __init__(self, simple: _Optional[_Union[BamlValuePromptAstSimple, _Mapping]] = ..., message: _Optional[_Union[BamlValuePromptAstMessage, _Mapping]] = ..., multiple: _Optional[_Union[BamlValuePromptAstMultiple, _Mapping]] = ...) -> None: ...

class BamlValuePromptAstMessage(_message.Message):
    __slots__ = ("role", "content", "metadata_as_json")
    ROLE_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    METADATA_AS_JSON_FIELD_NUMBER: _ClassVar[int]
    role: str
    content: BamlValuePromptAstSimple
    metadata_as_json: str
    def __init__(self, role: _Optional[str] = ..., content: _Optional[_Union[BamlValuePromptAstSimple, _Mapping]] = ..., metadata_as_json: _Optional[str] = ...) -> None: ...

class BamlValuePromptAstMultiple(_message.Message):
    __slots__ = ("items",)
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    items: _containers.RepeatedCompositeFieldContainer[BamlValuePromptAst]
    def __init__(self, items: _Optional[_Iterable[_Union[BamlValuePromptAst, _Mapping]]] = ...) -> None: ...

class BamlValuePromptAstSimple(_message.Message):
    __slots__ = ("string", "media", "multiple")
    STRING_FIELD_NUMBER: _ClassVar[int]
    MEDIA_FIELD_NUMBER: _ClassVar[int]
    MULTIPLE_FIELD_NUMBER: _ClassVar[int]
    string: str
    media: BamlValueMedia
    multiple: BamlValuePromptAstSimpleMultiple
    def __init__(self, string: _Optional[str] = ..., media: _Optional[_Union[BamlValueMedia, _Mapping]] = ..., multiple: _Optional[_Union[BamlValuePromptAstSimpleMultiple, _Mapping]] = ...) -> None: ...

class BamlValuePromptAstSimpleMultiple(_message.Message):
    __slots__ = ("items",)
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    items: _containers.RepeatedCompositeFieldContainer[BamlValuePromptAstSimple]
    def __init__(self, items: _Optional[_Iterable[_Union[BamlValuePromptAstSimple, _Mapping]]] = ...) -> None: ...

class BamlToHostCall(_message.Message):
    __slots__ = ("args",)
    ARGS_FIELD_NUMBER: _ClassVar[int]
    args: _containers.RepeatedCompositeFieldContainer[BamlToHostArg]
    def __init__(self, args: _Optional[_Iterable[_Union[BamlToHostArg, _Mapping]]] = ...) -> None: ...

class BamlToHostArg(_message.Message):
    __slots__ = ("value", "arg_name", "is_optional_arg")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ARG_NAME_FIELD_NUMBER: _ClassVar[int]
    IS_OPTIONAL_ARG_FIELD_NUMBER: _ClassVar[int]
    value: BamlOutboundValue
    arg_name: str
    is_optional_arg: bool
    def __init__(self, value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ..., arg_name: _Optional[str] = ..., is_optional_arg: bool = ...) -> None: ...

class BamlLiteralValue(_message.Message):
    __slots__ = ("string_value", "int_value", "bool_value", "bigint_value", "float_value")
    STRING_VALUE_FIELD_NUMBER: _ClassVar[int]
    INT_VALUE_FIELD_NUMBER: _ClassVar[int]
    BOOL_VALUE_FIELD_NUMBER: _ClassVar[int]
    BIGINT_VALUE_FIELD_NUMBER: _ClassVar[int]
    FLOAT_VALUE_FIELD_NUMBER: _ClassVar[int]
    string_value: str
    int_value: int
    bool_value: bool
    bigint_value: str
    float_value: str
    def __init__(self, string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., bool_value: bool = ..., bigint_value: _Optional[str] = ..., float_value: _Optional[str] = ...) -> None: ...
