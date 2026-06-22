from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class TyPrimitiveKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    TY_PRIMITIVE_UNSPECIFIED: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_STRING: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_INT: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_FLOAT: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_BOOL: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_NULL: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_BYTES: _ClassVar[TyPrimitiveKind]
    TY_PRIMITIVE_BIGINT: _ClassVar[TyPrimitiveKind]

class TyMediaKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    TY_MEDIA_KIND_UNSPECIFIED: _ClassVar[TyMediaKind]
    TY_MEDIA_KIND_IMAGE: _ClassVar[TyMediaKind]
    TY_MEDIA_KIND_AUDIO: _ClassVar[TyMediaKind]
    TY_MEDIA_KIND_VIDEO: _ClassVar[TyMediaKind]
    TY_MEDIA_KIND_PDF: _ClassVar[TyMediaKind]
    TY_MEDIA_KIND_GENERIC: _ClassVar[TyMediaKind]

class TyFunctionParamMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    TY_FUNCTION_PARAM_MODE_UNSPECIFIED: _ClassVar[TyFunctionParamMode]
    TY_FUNCTION_PARAM_MODE_REQUIRED: _ClassVar[TyFunctionParamMode]
    TY_FUNCTION_PARAM_MODE_OPTIONAL: _ClassVar[TyFunctionParamMode]
TY_PRIMITIVE_UNSPECIFIED: TyPrimitiveKind
TY_PRIMITIVE_STRING: TyPrimitiveKind
TY_PRIMITIVE_INT: TyPrimitiveKind
TY_PRIMITIVE_FLOAT: TyPrimitiveKind
TY_PRIMITIVE_BOOL: TyPrimitiveKind
TY_PRIMITIVE_NULL: TyPrimitiveKind
TY_PRIMITIVE_BYTES: TyPrimitiveKind
TY_PRIMITIVE_BIGINT: TyPrimitiveKind
TY_MEDIA_KIND_UNSPECIFIED: TyMediaKind
TY_MEDIA_KIND_IMAGE: TyMediaKind
TY_MEDIA_KIND_AUDIO: TyMediaKind
TY_MEDIA_KIND_VIDEO: TyMediaKind
TY_MEDIA_KIND_PDF: TyMediaKind
TY_MEDIA_KIND_GENERIC: TyMediaKind
TY_FUNCTION_PARAM_MODE_UNSPECIFIED: TyFunctionParamMode
TY_FUNCTION_PARAM_MODE_REQUIRED: TyFunctionParamMode
TY_FUNCTION_PARAM_MODE_OPTIONAL: TyFunctionParamMode

class Ty(_message.Message):
    __slots__ = ("primitive", "class_ty", "enum", "list", "map", "optional", "union", "literal", "type_alias", "unknown", "media", "interface", "enum_variant", "function", "future", "rust_type", "meta_type", "resource", "prompt_ast", "void", "watch_accessor", "type_var", "associated_type_projection", "never")
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
    WATCH_ACCESSOR_FIELD_NUMBER: _ClassVar[int]
    TYPE_VAR_FIELD_NUMBER: _ClassVar[int]
    ASSOCIATED_TYPE_PROJECTION_FIELD_NUMBER: _ClassVar[int]
    NEVER_FIELD_NUMBER: _ClassVar[int]
    primitive: TyPrimitive
    class_ty: TyClass
    enum: TyEnum
    list: TyList
    map: TyMap
    optional: TyOptional
    union: TyUnion
    literal: TyLiteral
    type_alias: TyTypeAlias
    unknown: TyUnknown
    media: TyMedia
    interface: TyInterface
    enum_variant: TyEnumVariant
    function: TyFunction
    future: TyFuture
    rust_type: TyRustType
    meta_type: TyMetaType
    resource: TyResource
    prompt_ast: TyPromptAst
    void: TyVoid
    watch_accessor: TyWatchAccessor
    type_var: TyTypeVar
    associated_type_projection: TyAssociatedTypeProjection
    never: TyNever
    def __init__(self, primitive: _Optional[_Union[TyPrimitive, _Mapping]] = ..., class_ty: _Optional[_Union[TyClass, _Mapping]] = ..., enum: _Optional[_Union[TyEnum, _Mapping]] = ..., list: _Optional[_Union[TyList, _Mapping]] = ..., map: _Optional[_Union[TyMap, _Mapping]] = ..., optional: _Optional[_Union[TyOptional, _Mapping]] = ..., union: _Optional[_Union[TyUnion, _Mapping]] = ..., literal: _Optional[_Union[TyLiteral, _Mapping]] = ..., type_alias: _Optional[_Union[TyTypeAlias, _Mapping]] = ..., unknown: _Optional[_Union[TyUnknown, _Mapping]] = ..., media: _Optional[_Union[TyMedia, _Mapping]] = ..., interface: _Optional[_Union[TyInterface, _Mapping]] = ..., enum_variant: _Optional[_Union[TyEnumVariant, _Mapping]] = ..., function: _Optional[_Union[TyFunction, _Mapping]] = ..., future: _Optional[_Union[TyFuture, _Mapping]] = ..., rust_type: _Optional[_Union[TyRustType, _Mapping]] = ..., meta_type: _Optional[_Union[TyMetaType, _Mapping]] = ..., resource: _Optional[_Union[TyResource, _Mapping]] = ..., prompt_ast: _Optional[_Union[TyPromptAst, _Mapping]] = ..., void: _Optional[_Union[TyVoid, _Mapping]] = ..., watch_accessor: _Optional[_Union[TyWatchAccessor, _Mapping]] = ..., type_var: _Optional[_Union[TyTypeVar, _Mapping]] = ..., associated_type_projection: _Optional[_Union[TyAssociatedTypeProjection, _Mapping]] = ..., never: _Optional[_Union[TyNever, _Mapping]] = ...) -> None: ...

class TyPrimitive(_message.Message):
    __slots__ = ("kind",)
    KIND_FIELD_NUMBER: _ClassVar[int]
    kind: TyPrimitiveKind
    def __init__(self, kind: _Optional[_Union[TyPrimitiveKind, str]] = ...) -> None: ...

class TyClass(_message.Message):
    __slots__ = ("name", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[Ty]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[Ty, _Mapping]]] = ...) -> None: ...

class TyTypeAlias(_message.Message):
    __slots__ = ("name", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[Ty]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[Ty, _Mapping]]] = ...) -> None: ...

class TyEnum(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class TyList(_message.Message):
    __slots__ = ("item",)
    ITEM_FIELD_NUMBER: _ClassVar[int]
    item: Ty
    def __init__(self, item: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyMap(_message.Message):
    __slots__ = ("key", "value")
    KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    key: Ty
    value: Ty
    def __init__(self, key: _Optional[_Union[Ty, _Mapping]] = ..., value: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyOptional(_message.Message):
    __slots__ = ("inner",)
    INNER_FIELD_NUMBER: _ClassVar[int]
    inner: Ty
    def __init__(self, inner: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyUnion(_message.Message):
    __slots__ = ("options",)
    OPTIONS_FIELD_NUMBER: _ClassVar[int]
    options: _containers.RepeatedCompositeFieldContainer[Ty]
    def __init__(self, options: _Optional[_Iterable[_Union[Ty, _Mapping]]] = ...) -> None: ...

class TyUnknown(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyLiteral(_message.Message):
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

class TyMedia(_message.Message):
    __slots__ = ("kind",)
    KIND_FIELD_NUMBER: _ClassVar[int]
    kind: TyMediaKind
    def __init__(self, kind: _Optional[_Union[TyMediaKind, str]] = ...) -> None: ...

class TyInterface(_message.Message):
    __slots__ = ("name", "type_args", "bindings")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    BINDINGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    type_args: _containers.RepeatedCompositeFieldContainer[Ty]
    bindings: _containers.RepeatedCompositeFieldContainer[TyAssociatedBinding]
    def __init__(self, name: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[Ty, _Mapping]]] = ..., bindings: _Optional[_Iterable[_Union[TyAssociatedBinding, _Mapping]]] = ...) -> None: ...

class TyAssociatedBinding(_message.Message):
    __slots__ = ("name", "ty")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    name: str
    ty: Ty
    def __init__(self, name: _Optional[str] = ..., ty: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyEnumVariant(_message.Message):
    __slots__ = ("name", "variant")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VARIANT_FIELD_NUMBER: _ClassVar[int]
    name: str
    variant: str
    def __init__(self, name: _Optional[str] = ..., variant: _Optional[str] = ...) -> None: ...

class TyFunctionParam(_message.Message):
    __slots__ = ("name", "ty", "mode")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TY_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    name: str
    ty: Ty
    mode: TyFunctionParamMode
    def __init__(self, name: _Optional[str] = ..., ty: _Optional[_Union[Ty, _Mapping]] = ..., mode: _Optional[_Union[TyFunctionParamMode, str]] = ...) -> None: ...

class TyGenericParamBound(_message.Message):
    __slots__ = ("ty",)
    TY_FIELD_NUMBER: _ClassVar[int]
    ty: Ty
    def __init__(self, ty: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyFunction(_message.Message):
    __slots__ = ("generic_params", "generic_param_bounds", "params", "ret", "throws")
    GENERIC_PARAMS_FIELD_NUMBER: _ClassVar[int]
    GENERIC_PARAM_BOUNDS_FIELD_NUMBER: _ClassVar[int]
    PARAMS_FIELD_NUMBER: _ClassVar[int]
    RET_FIELD_NUMBER: _ClassVar[int]
    THROWS_FIELD_NUMBER: _ClassVar[int]
    generic_params: _containers.RepeatedScalarFieldContainer[str]
    generic_param_bounds: _containers.RepeatedCompositeFieldContainer[TyGenericParamBound]
    params: _containers.RepeatedCompositeFieldContainer[TyFunctionParam]
    ret: Ty
    throws: Ty
    def __init__(self, generic_params: _Optional[_Iterable[str]] = ..., generic_param_bounds: _Optional[_Iterable[_Union[TyGenericParamBound, _Mapping]]] = ..., params: _Optional[_Iterable[_Union[TyFunctionParam, _Mapping]]] = ..., ret: _Optional[_Union[Ty, _Mapping]] = ..., throws: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyFuture(_message.Message):
    __slots__ = ("value", "error")
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    value: Ty
    error: Ty
    def __init__(self, value: _Optional[_Union[Ty, _Mapping]] = ..., error: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyRustType(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyMetaType(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyResource(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyPromptAst(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyVoid(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TyWatchAccessor(_message.Message):
    __slots__ = ("inner",)
    INNER_FIELD_NUMBER: _ClassVar[int]
    inner: Ty
    def __init__(self, inner: _Optional[_Union[Ty, _Mapping]] = ...) -> None: ...

class TyTypeVar(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class TyAssociatedTypeProjection(_message.Message):
    __slots__ = ("base", "interface", "member")
    BASE_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_FIELD_NUMBER: _ClassVar[int]
    MEMBER_FIELD_NUMBER: _ClassVar[int]
    base: Ty
    interface: Ty
    member: str
    def __init__(self, base: _Optional[_Union[Ty, _Mapping]] = ..., interface: _Optional[_Union[Ty, _Mapping]] = ..., member: _Optional[str] = ...) -> None: ...

class TyNever(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...
