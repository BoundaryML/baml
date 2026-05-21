from baml_core.cffi.v1 import baml_inbound_pb2 as _baml_inbound_pb2
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

class BamlOutboundValue(_message.Message):
    __slots__ = ("null_value", "string_value", "int_value", "float_value", "bool_value", "class_value", "enum_value", "literal_value", "list_value", "map_value", "union_variant_value", "handle_value", "media_value", "prompt_ast_value", "uint8array_value")
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
    null_value: BamlValueNull
    string_value: str
    int_value: int
    float_value: float
    bool_value: bool
    class_value: BamlValueClass
    enum_value: BamlValueEnum
    literal_value: BamlTyLiteral
    list_value: BamlValueList
    map_value: BamlValueMap
    union_variant_value: BamlValueUnionVariant
    handle_value: BamlOutboundHandle
    media_value: BamlValueMedia
    prompt_ast_value: BamlValuePromptAst
    uint8array_value: bytes
    def __init__(self, null_value: _Optional[_Union[BamlValueNull, _Mapping]] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., class_value: _Optional[_Union[BamlValueClass, _Mapping]] = ..., enum_value: _Optional[_Union[BamlValueEnum, _Mapping]] = ..., literal_value: _Optional[_Union[BamlTyLiteral, _Mapping]] = ..., list_value: _Optional[_Union[BamlValueList, _Mapping]] = ..., map_value: _Optional[_Union[BamlValueMap, _Mapping]] = ..., union_variant_value: _Optional[_Union[BamlValueUnionVariant, _Mapping]] = ..., handle_value: _Optional[_Union[BamlOutboundHandle, _Mapping]] = ..., media_value: _Optional[_Union[BamlValueMedia, _Mapping]] = ..., prompt_ast_value: _Optional[_Union[BamlValuePromptAst, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ...) -> None: ...

class BamlOutboundHandle(_message.Message):
    __slots__ = ("key", "handle_type", "name")
    KEY_FIELD_NUMBER: _ClassVar[int]
    HANDLE_TYPE_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    key: int
    handle_type: _baml_inbound_pb2.BamlHandleType
    name: BamlTyName
    def __init__(self, key: _Optional[int] = ..., handle_type: _Optional[_Union[_baml_inbound_pb2.BamlHandleType, str]] = ..., name: _Optional[_Union[BamlTyName, _Mapping]] = ...) -> None: ...

class BamlTyName(_message.Message):
    __slots__ = ("name", "generic_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    GENERIC_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    generic_args: _containers.RepeatedCompositeFieldContainer[BamlTyGenericArg]
    def __init__(self, name: _Optional[str] = ..., generic_args: _Optional[_Iterable[_Union[BamlTyGenericArg, _Mapping]]] = ...) -> None: ...

class BamlTyGenericArg(_message.Message):
    __slots__ = ("name", "ty")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    name: str
    ty: BamlTy
    def __init__(self, name: _Optional[str] = ..., ty: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlValueNull(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlValueList(_message.Message):
    __slots__ = ("item_type", "items")
    ITEM_TYPE_FIELD_NUMBER: _ClassVar[int]
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    item_type: BamlTy
    items: _containers.RepeatedCompositeFieldContainer[BamlOutboundValue]
    def __init__(self, item_type: _Optional[_Union[BamlTy, _Mapping]] = ..., items: _Optional[_Iterable[_Union[BamlOutboundValue, _Mapping]]] = ...) -> None: ...

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
    key_type: BamlTy
    value_type: BamlTy
    entries: _containers.RepeatedCompositeFieldContainer[BamlOutboundMapEntry]
    def __init__(self, key_type: _Optional[_Union[BamlTy, _Mapping]] = ..., value_type: _Optional[_Union[BamlTy, _Mapping]] = ..., entries: _Optional[_Iterable[_Union[BamlOutboundMapEntry, _Mapping]]] = ...) -> None: ...

class BamlValueClass(_message.Message):
    __slots__ = ("name", "fields")
    NAME_FIELD_NUMBER: _ClassVar[int]
    FIELDS_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    fields: _containers.RepeatedCompositeFieldContainer[BamlOutboundMapEntry]
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ..., fields: _Optional[_Iterable[_Union[BamlOutboundMapEntry, _Mapping]]] = ...) -> None: ...

class BamlValueEnum(_message.Message):
    __slots__ = ("name", "value", "is_dynamic")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    IS_DYNAMIC_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    value: str
    is_dynamic: bool
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ..., value: _Optional[str] = ..., is_dynamic: bool = ...) -> None: ...

class BamlValueUnionVariant(_message.Message):
    __slots__ = ("name", "is_optional", "is_single_pattern", "self_type", "value_option_name", "value")
    NAME_FIELD_NUMBER: _ClassVar[int]
    IS_OPTIONAL_FIELD_NUMBER: _ClassVar[int]
    IS_SINGLE_PATTERN_FIELD_NUMBER: _ClassVar[int]
    SELF_TYPE_FIELD_NUMBER: _ClassVar[int]
    VALUE_OPTION_NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    is_optional: bool
    is_single_pattern: bool
    self_type: BamlTy
    value_option_name: str
    value: BamlOutboundValue
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ..., is_optional: bool = ..., is_single_pattern: bool = ..., self_type: _Optional[_Union[BamlTy, _Mapping]] = ..., value_option_name: _Optional[str] = ..., value: _Optional[_Union[BamlOutboundValue, _Mapping]] = ...) -> None: ...

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

class BamlTy(_message.Message):
    __slots__ = ("string_type", "int_type", "float_type", "bool_type", "null_type", "literal_type", "media_type", "enum_type", "class_type", "type_alias_type", "list_type", "map_type", "union_variant_type", "optional_type", "any_type", "uint8array_type", "unknown_type")
    STRING_TYPE_FIELD_NUMBER: _ClassVar[int]
    INT_TYPE_FIELD_NUMBER: _ClassVar[int]
    FLOAT_TYPE_FIELD_NUMBER: _ClassVar[int]
    BOOL_TYPE_FIELD_NUMBER: _ClassVar[int]
    NULL_TYPE_FIELD_NUMBER: _ClassVar[int]
    LITERAL_TYPE_FIELD_NUMBER: _ClassVar[int]
    MEDIA_TYPE_FIELD_NUMBER: _ClassVar[int]
    ENUM_TYPE_FIELD_NUMBER: _ClassVar[int]
    CLASS_TYPE_FIELD_NUMBER: _ClassVar[int]
    TYPE_ALIAS_TYPE_FIELD_NUMBER: _ClassVar[int]
    LIST_TYPE_FIELD_NUMBER: _ClassVar[int]
    MAP_TYPE_FIELD_NUMBER: _ClassVar[int]
    UNION_VARIANT_TYPE_FIELD_NUMBER: _ClassVar[int]
    OPTIONAL_TYPE_FIELD_NUMBER: _ClassVar[int]
    ANY_TYPE_FIELD_NUMBER: _ClassVar[int]
    UINT8ARRAY_TYPE_FIELD_NUMBER: _ClassVar[int]
    UNKNOWN_TYPE_FIELD_NUMBER: _ClassVar[int]
    string_type: BamlTyString
    int_type: BamlTyInt
    float_type: BamlTyFloat
    bool_type: BamlTyBool
    null_type: BamlTyNull
    literal_type: BamlTyLiteral
    media_type: BamlTyMedia
    enum_type: BamlTyEnum
    class_type: BamlTyClass
    type_alias_type: BamlTyTypeAlias
    list_type: BamlTyList
    map_type: BamlTyMap
    union_variant_type: BamlTyUnionVariant
    optional_type: BamlTyOptional
    any_type: BamlTyAny
    uint8array_type: BamlTyUint8Array
    unknown_type: BamlTyUnknown
    def __init__(self, string_type: _Optional[_Union[BamlTyString, _Mapping]] = ..., int_type: _Optional[_Union[BamlTyInt, _Mapping]] = ..., float_type: _Optional[_Union[BamlTyFloat, _Mapping]] = ..., bool_type: _Optional[_Union[BamlTyBool, _Mapping]] = ..., null_type: _Optional[_Union[BamlTyNull, _Mapping]] = ..., literal_type: _Optional[_Union[BamlTyLiteral, _Mapping]] = ..., media_type: _Optional[_Union[BamlTyMedia, _Mapping]] = ..., enum_type: _Optional[_Union[BamlTyEnum, _Mapping]] = ..., class_type: _Optional[_Union[BamlTyClass, _Mapping]] = ..., type_alias_type: _Optional[_Union[BamlTyTypeAlias, _Mapping]] = ..., list_type: _Optional[_Union[BamlTyList, _Mapping]] = ..., map_type: _Optional[_Union[BamlTyMap, _Mapping]] = ..., union_variant_type: _Optional[_Union[BamlTyUnionVariant, _Mapping]] = ..., optional_type: _Optional[_Union[BamlTyOptional, _Mapping]] = ..., any_type: _Optional[_Union[BamlTyAny, _Mapping]] = ..., uint8array_type: _Optional[_Union[BamlTyUint8Array, _Mapping]] = ..., unknown_type: _Optional[_Union[BamlTyUnknown, _Mapping]] = ...) -> None: ...

class BamlTyString(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyInt(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyFloat(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyBool(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyNull(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyUint8Array(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyAny(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyUnknown(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlLiteralString(_message.Message):
    __slots__ = ("value",)
    VALUE_FIELD_NUMBER: _ClassVar[int]
    value: str
    def __init__(self, value: _Optional[str] = ...) -> None: ...

class BamlLiteralInt(_message.Message):
    __slots__ = ("value",)
    VALUE_FIELD_NUMBER: _ClassVar[int]
    value: int
    def __init__(self, value: _Optional[int] = ...) -> None: ...

class BamlLiteralBool(_message.Message):
    __slots__ = ("value",)
    VALUE_FIELD_NUMBER: _ClassVar[int]
    value: bool
    def __init__(self, value: bool = ...) -> None: ...

class BamlTyLiteral(_message.Message):
    __slots__ = ("string_literal", "int_literal", "bool_literal")
    STRING_LITERAL_FIELD_NUMBER: _ClassVar[int]
    INT_LITERAL_FIELD_NUMBER: _ClassVar[int]
    BOOL_LITERAL_FIELD_NUMBER: _ClassVar[int]
    string_literal: BamlLiteralString
    int_literal: BamlLiteralInt
    bool_literal: BamlLiteralBool
    def __init__(self, string_literal: _Optional[_Union[BamlLiteralString, _Mapping]] = ..., int_literal: _Optional[_Union[BamlLiteralInt, _Mapping]] = ..., bool_literal: _Optional[_Union[BamlLiteralBool, _Mapping]] = ...) -> None: ...

class BamlTyMedia(_message.Message):
    __slots__ = ("media",)
    MEDIA_FIELD_NUMBER: _ClassVar[int]
    media: MediaTypeEnum
    def __init__(self, media: _Optional[_Union[MediaTypeEnum, str]] = ...) -> None: ...

class BamlTyEnum(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class BamlTyClass(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ...) -> None: ...

class BamlTyTypeAlias(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ...) -> None: ...

class BamlTyList(_message.Message):
    __slots__ = ("item_type",)
    ITEM_TYPE_FIELD_NUMBER: _ClassVar[int]
    item_type: BamlTy
    def __init__(self, item_type: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyMap(_message.Message):
    __slots__ = ("key_type", "value_type")
    KEY_TYPE_FIELD_NUMBER: _ClassVar[int]
    VALUE_TYPE_FIELD_NUMBER: _ClassVar[int]
    key_type: BamlTy
    value_type: BamlTy
    def __init__(self, key_type: _Optional[_Union[BamlTy, _Mapping]] = ..., value_type: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyUnionVariant(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: BamlTyName
    def __init__(self, name: _Optional[_Union[BamlTyName, _Mapping]] = ...) -> None: ...

class BamlTyOptional(_message.Message):
    __slots__ = ("value",)
    VALUE_FIELD_NUMBER: _ClassVar[int]
    value: BamlTy
    def __init__(self, value: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...
