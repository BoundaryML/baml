from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class BamlTyPrimitiveKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    BAML_TY_PRIMITIVE_UNSPECIFIED: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_STRING: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_INT: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_FLOAT: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_BOOL: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_NULL: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_BYTES: _ClassVar[BamlTyPrimitiveKind]
    BAML_TY_PRIMITIVE_BIGINT: _ClassVar[BamlTyPrimitiveKind]

class BamlTyMediaKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    BAML_TY_MEDIA_KIND_UNSPECIFIED: _ClassVar[BamlTyMediaKind]
    BAML_TY_MEDIA_KIND_IMAGE: _ClassVar[BamlTyMediaKind]
    BAML_TY_MEDIA_KIND_AUDIO: _ClassVar[BamlTyMediaKind]
    BAML_TY_MEDIA_KIND_VIDEO: _ClassVar[BamlTyMediaKind]
    BAML_TY_MEDIA_KIND_PDF: _ClassVar[BamlTyMediaKind]
    BAML_TY_MEDIA_KIND_GENERIC: _ClassVar[BamlTyMediaKind]

class BamlTyFunctionParamMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    BAML_TY_FUNCTION_PARAM_MODE_UNSPECIFIED: _ClassVar[BamlTyFunctionParamMode]
    BAML_TY_FUNCTION_PARAM_MODE_REQUIRED: _ClassVar[BamlTyFunctionParamMode]
    BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL: _ClassVar[BamlTyFunctionParamMode]
BAML_TY_PRIMITIVE_UNSPECIFIED: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_STRING: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_INT: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_FLOAT: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_BOOL: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_NULL: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_BYTES: BamlTyPrimitiveKind
BAML_TY_PRIMITIVE_BIGINT: BamlTyPrimitiveKind
BAML_TY_MEDIA_KIND_UNSPECIFIED: BamlTyMediaKind
BAML_TY_MEDIA_KIND_IMAGE: BamlTyMediaKind
BAML_TY_MEDIA_KIND_AUDIO: BamlTyMediaKind
BAML_TY_MEDIA_KIND_VIDEO: BamlTyMediaKind
BAML_TY_MEDIA_KIND_PDF: BamlTyMediaKind
BAML_TY_MEDIA_KIND_GENERIC: BamlTyMediaKind
BAML_TY_FUNCTION_PARAM_MODE_UNSPECIFIED: BamlTyFunctionParamMode
BAML_TY_FUNCTION_PARAM_MODE_REQUIRED: BamlTyFunctionParamMode
BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL: BamlTyFunctionParamMode

class BamlTy(_message.Message):
    __slots__ = ("primitive", "class_ty", "enum", "list", "map", "optional", "union", "literal", "type_alias", "unknown", "media", "interface", "enum_variant", "function", "future", "rust_type", "meta_type", "resource", "prompt_ast", "void", "type_var", "associated_type_projection", "never")
    PRIMITIVE_FIELD_NUMBER: _ClassVar[int]
    CLASS_TY_FIELD_NUMBER: _ClassVar[int]
    ENUM_FIELD_NUMBER: _ClassVar[int]
    LIST_FIELD_NUMBER: _ClassVar[int]
    MAP_FIELD_NUMBER: _ClassVar[int]
    OPTIONAL_FIELD_NUMBER: _ClassVar[int]
    UNION_FIELD_NUMBER: _ClassVar[int]
    LITERAL_FIELD_NUMBER: _ClassVar[int]
    TYPE_ALIAS_FIELD_NUMBER: _ClassVar[int]
    UNKNOWN_FIELD_NUMBER: _ClassVar[int]
    MEDIA_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_FIELD_NUMBER: _ClassVar[int]
    ENUM_VARIANT_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_FIELD_NUMBER: _ClassVar[int]
    FUTURE_FIELD_NUMBER: _ClassVar[int]
    RUST_TYPE_FIELD_NUMBER: _ClassVar[int]
    META_TYPE_FIELD_NUMBER: _ClassVar[int]
    RESOURCE_FIELD_NUMBER: _ClassVar[int]
    PROMPT_AST_FIELD_NUMBER: _ClassVar[int]
    VOID_FIELD_NUMBER: _ClassVar[int]
    TYPE_VAR_FIELD_NUMBER: _ClassVar[int]
    ASSOCIATED_TYPE_PROJECTION_FIELD_NUMBER: _ClassVar[int]
    NEVER_FIELD_NUMBER: _ClassVar[int]
    primitive: BamlTyPrimitive
    class_ty: BamlTyClass
    enum: BamlTyEnum
    list: BamlTyList
    map: BamlTyMap
    optional: BamlTyOptional
    union: BamlTyUnion
    literal: BamlTyLiteral
    type_alias: BamlTyTypeAlias
    unknown: BamlTyUnknown
    media: BamlTyMedia
    interface: BamlTyInterface
    enum_variant: BamlTyEnumVariant
    function: BamlTyFunction
    future: BamlTyFuture
    rust_type: BamlTyRustType
    meta_type: BamlTyMetaType
    resource: BamlTyResource
    prompt_ast: BamlTyPromptAst
    void: BamlTyVoid
    type_var: BamlTyTypeVar
    associated_type_projection: BamlTyAssociatedTypeProjection
    never: BamlTyNever
    def __init__(self, primitive: _Optional[_Union[BamlTyPrimitive, _Mapping]] = ..., class_ty: _Optional[_Union[BamlTyClass, _Mapping]] = ..., enum: _Optional[_Union[BamlTyEnum, _Mapping]] = ..., list: _Optional[_Union[BamlTyList, _Mapping]] = ..., map: _Optional[_Union[BamlTyMap, _Mapping]] = ..., optional: _Optional[_Union[BamlTyOptional, _Mapping]] = ..., union: _Optional[_Union[BamlTyUnion, _Mapping]] = ..., literal: _Optional[_Union[BamlTyLiteral, _Mapping]] = ..., type_alias: _Optional[_Union[BamlTyTypeAlias, _Mapping]] = ..., unknown: _Optional[_Union[BamlTyUnknown, _Mapping]] = ..., media: _Optional[_Union[BamlTyMedia, _Mapping]] = ..., interface: _Optional[_Union[BamlTyInterface, _Mapping]] = ..., enum_variant: _Optional[_Union[BamlTyEnumVariant, _Mapping]] = ..., function: _Optional[_Union[BamlTyFunction, _Mapping]] = ..., future: _Optional[_Union[BamlTyFuture, _Mapping]] = ..., rust_type: _Optional[_Union[BamlTyRustType, _Mapping]] = ..., meta_type: _Optional[_Union[BamlTyMetaType, _Mapping]] = ..., resource: _Optional[_Union[BamlTyResource, _Mapping]] = ..., prompt_ast: _Optional[_Union[BamlTyPromptAst, _Mapping]] = ..., void: _Optional[_Union[BamlTyVoid, _Mapping]] = ..., type_var: _Optional[_Union[BamlTyTypeVar, _Mapping]] = ..., associated_type_projection: _Optional[_Union[BamlTyAssociatedTypeProjection, _Mapping]] = ..., never: _Optional[_Union[BamlTyNever, _Mapping]] = ...) -> None: ...

class BamlTyPrimitive(_message.Message):
    __slots__ = ("kind",)
    KIND_FIELD_NUMBER: _ClassVar[int]
    kind: BamlTyPrimitiveKind
    def __init__(self, kind: _Optional[_Union[BamlTyPrimitiveKind, str]] = ...) -> None: ...

class BamlTyClass(_message.Message):
    __slots__ = ("name", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTy]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[BamlTy, _Mapping]]] = ...) -> None: ...

class BamlTyTypeAlias(_message.Message):
    __slots__ = ("name", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTy]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[BamlTy, _Mapping]]] = ...) -> None: ...

class BamlTyEnum(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class BamlTyList(_message.Message):
    __slots__ = ("item",)
    ITEM_FIELD_NUMBER: _ClassVar[int]
    item: BamlTy
    def __init__(self, item: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyMap(_message.Message):
    __slots__ = ("key", "value")
    KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    key: BamlTy
    value: BamlTy
    def __init__(self, key: _Optional[_Union[BamlTy, _Mapping]] = ..., value: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyOptional(_message.Message):
    __slots__ = ("inner",)
    INNER_FIELD_NUMBER: _ClassVar[int]
    inner: BamlTy
    def __init__(self, inner: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyUnion(_message.Message):
    __slots__ = ("options",)
    OPTIONS_FIELD_NUMBER: _ClassVar[int]
    options: _containers.RepeatedCompositeFieldContainer[BamlTy]
    def __init__(self, options: _Optional[_Iterable[_Union[BamlTy, _Mapping]]] = ...) -> None: ...

class BamlTyUnknown(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyLiteral(_message.Message):
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

class BamlTyMedia(_message.Message):
    __slots__ = ("kind",)
    KIND_FIELD_NUMBER: _ClassVar[int]
    kind: BamlTyMediaKind
    def __init__(self, kind: _Optional[_Union[BamlTyMediaKind, str]] = ...) -> None: ...

class BamlTyInterface(_message.Message):
    __slots__ = ("name", "type_args", "bindings")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    BINDINGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTy]
    bindings: _containers.RepeatedCompositeFieldContainer[BamlTyAssociatedBinding]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[BamlTy, _Mapping]]] = ..., bindings: _Optional[_Iterable[_Union[BamlTyAssociatedBinding, _Mapping]]] = ...) -> None: ...

class BamlTyAssociatedBinding(_message.Message):
    __slots__ = ("name", "ty")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    name: str
    ty: BamlTy
    def __init__(self, name: _Optional[str] = ..., ty: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyEnumVariant(_message.Message):
    __slots__ = ("name", "variant")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VARIANT_FIELD_NUMBER: _ClassVar[int]
    name: str
    variant: str
    def __init__(self, name: _Optional[str] = ..., variant: _Optional[str] = ...) -> None: ...

class BamlTyFunctionParam(_message.Message):
    __slots__ = ("name", "ty", "mode")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    name: str
    ty: BamlTy
    mode: BamlTyFunctionParamMode
    def __init__(self, name: _Optional[str] = ..., ty: _Optional[_Union[BamlTy, _Mapping]] = ..., mode: _Optional[_Union[BamlTyFunctionParamMode, str]] = ...) -> None: ...

class BamlTyFunction(_message.Message):
    __slots__ = ("generic_params", "params", "ret", "throws")
    GENERIC_PARAMS_FIELD_NUMBER: _ClassVar[int]
    PARAMS_FIELD_NUMBER: _ClassVar[int]
    RET_FIELD_NUMBER: _ClassVar[int]
    THROWS_FIELD_NUMBER: _ClassVar[int]
    generic_params: _containers.RepeatedScalarFieldContainer[str]
    params: _containers.RepeatedCompositeFieldContainer[BamlTyFunctionParam]
    ret: BamlTy
    throws: BamlTy
    def __init__(self, generic_params: _Optional[_Iterable[str]] = ..., params: _Optional[_Iterable[_Union[BamlTyFunctionParam, _Mapping]]] = ..., ret: _Optional[_Union[BamlTy, _Mapping]] = ..., throws: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyFuture(_message.Message):
    __slots__ = ("value", "error")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    value: BamlTy
    error: BamlTy
    def __init__(self, value: _Optional[_Union[BamlTy, _Mapping]] = ..., error: _Optional[_Union[BamlTy, _Mapping]] = ...) -> None: ...

class BamlTyRustType(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyMetaType(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyResource(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyPromptAst(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyVoid(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class BamlTyTypeVar(_message.Message):
    __slots__ = ("name", "index")
    NAME_FIELD_NUMBER: _ClassVar[int]
    INDEX_FIELD_NUMBER: _ClassVar[int]
    name: str
    index: int
    def __init__(self, name: _Optional[str] = ..., index: _Optional[int] = ...) -> None: ...

class BamlTyAssociatedTypeProjection(_message.Message):
    __slots__ = ("base", "interface", "member")
    BASE_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_FIELD_NUMBER: _ClassVar[int]
    MEMBER_FIELD_NUMBER: _ClassVar[int]
    base: BamlTy
    interface: BamlTy
    member: str
    def __init__(self, base: _Optional[_Union[BamlTy, _Mapping]] = ..., interface: _Optional[_Union[BamlTy, _Mapping]] = ..., member: _Optional[str] = ...) -> None: ...

class BamlTyNever(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...
