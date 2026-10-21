from baml_bridge.cffi.v1 import baml_handle_pb2 as _baml_handle_pb2
from baml_bridge.cffi.v1 import baml_type_pb2 as _baml_type_pb2
from baml_bridge.cffi.v1 import baml_outbound_pb2 as _baml_outbound_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class TraceMode(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    TRACE_MODE_UNSPECIFIED: _ClassVar[TraceMode]
    TRACE_MODE_HIDDEN: _ClassVar[TraceMode]
    TRACE_MODE_TIMING: _ClassVar[TraceMode]
    TRACE_MODE_SPAN: _ClassVar[TraceMode]
TRACE_MODE_UNSPECIFIED: TraceMode
TRACE_MODE_HIDDEN: TraceMode
TRACE_MODE_TIMING: TraceMode
TRACE_MODE_SPAN: TraceMode

class InboundValue(_message.Message):
    __slots__ = ("value_type", "string_value", "int_value", "float_value", "bool_value", "list_value", "map_value", "class_value", "enum_value", "handle", "uint8array_value", "bigint_value", "ty_value", "ty_def_value", "media_value", "prompt_ast_value", "js_number_value")
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
    TY_DEF_VALUE_FIELD_NUMBER: _ClassVar[int]
    MEDIA_VALUE_FIELD_NUMBER: _ClassVar[int]
    PROMPT_AST_VALUE_FIELD_NUMBER: _ClassVar[int]
    JS_NUMBER_VALUE_FIELD_NUMBER: _ClassVar[int]
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
    ty_def_value: _baml_type_pb2.BamlTyDef
    media_value: _baml_outbound_pb2.BamlValueMedia
    prompt_ast_value: _baml_outbound_pb2.BamlValuePromptAst
    js_number_value: float
    def __init__(self, value_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., list_value: _Optional[_Union[InboundListValue, _Mapping]] = ..., map_value: _Optional[_Union[InboundMapValue, _Mapping]] = ..., class_value: _Optional[_Union[InboundClassValue, _Mapping]] = ..., enum_value: _Optional[_Union[InboundEnumValue, _Mapping]] = ..., handle: _Optional[_Union[_baml_handle_pb2.BamlHandle, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ..., bigint_value: _Optional[str] = ..., ty_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., ty_def_value: _Optional[_Union[_baml_type_pb2.BamlTyDef, _Mapping]] = ..., media_value: _Optional[_Union[_baml_outbound_pb2.BamlValueMedia, _Mapping]] = ..., prompt_ast_value: _Optional[_Union[_baml_outbound_pb2.BamlValuePromptAst, _Mapping]] = ..., js_number_value: _Optional[float] = ...) -> None: ...

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
    __slots__ = ("type_var", "type_value", "type_definition")
    TYPE_VAR_FIELD_NUMBER: _ClassVar[int]
    TYPE_VALUE_FIELD_NUMBER: _ClassVar[int]
    TYPE_DEFINITION_FIELD_NUMBER: _ClassVar[int]
    type_var: str
    type_value: _baml_type_pb2.BamlTy
    type_definition: _baml_type_pb2.BamlTyDef
    def __init__(self, type_var: _Optional[str] = ..., type_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., type_definition: _Optional[_Union[_baml_type_pb2.BamlTyDef, _Mapping]] = ...) -> None: ...

class CallFunctionArgs(_message.Message):
    __slots__ = ("kwargs", "call_id", "type_args", "function_name", "function_handle", "invocation")
    KWARGS_FIELD_NUMBER: _ClassVar[int]
    CALL_ID_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_NAME_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_HANDLE_FIELD_NUMBER: _ClassVar[int]
    INVOCATION_FIELD_NUMBER: _ClassVar[int]
    kwargs: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    call_id: int
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    function_name: str
    function_handle: int
    invocation: InvocationOptions
    def __init__(self, kwargs: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ..., call_id: _Optional[int] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ..., function_name: _Optional[str] = ..., function_handle: _Optional[int] = ..., invocation: _Optional[_Union[InvocationOptions, _Mapping]] = ...) -> None: ...

class InvocationOptions(_message.Message):
    __slots__ = ("trace", "cancel", "deadline_ns", "inherited_state", "host_environment")
    TRACE_FIELD_NUMBER: _ClassVar[int]
    CANCEL_FIELD_NUMBER: _ClassVar[int]
    DEADLINE_NS_FIELD_NUMBER: _ClassVar[int]
    INHERITED_STATE_FIELD_NUMBER: _ClassVar[int]
    HOST_ENVIRONMENT_FIELD_NUMBER: _ClassVar[int]
    trace: TraceSelection
    cancel: InboundValue
    deadline_ns: int
    inherited_state: int
    host_environment: int
    def __init__(self, trace: _Optional[_Union[TraceSelection, _Mapping]] = ..., cancel: _Optional[_Union[InboundValue, _Mapping]] = ..., deadline_ns: _Optional[int] = ..., inherited_state: _Optional[int] = ..., host_environment: _Optional[int] = ...) -> None: ...

class TraceSelection(_message.Message):
    __slots__ = ("options", "reservation")
    OPTIONS_FIELD_NUMBER: _ClassVar[int]
    RESERVATION_FIELD_NUMBER: _ClassVar[int]
    options: TraceOptions
    reservation: int
    def __init__(self, options: _Optional[_Union[TraceOptions, _Mapping]] = ..., reservation: _Optional[int] = ...) -> None: ...

class TraceOptions(_message.Message):
    __slots__ = ("mode", "inputs", "output", "error", "distinct_id", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: TraceMetadataValue
        def __init__(self, key: _Optional[str] = ..., value: _Optional[_Union[TraceMetadataValue, _Mapping]] = ...) -> None: ...
    MODE_FIELD_NUMBER: _ClassVar[int]
    INPUTS_FIELD_NUMBER: _ClassVar[int]
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    DISTINCT_ID_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    mode: TraceMode
    inputs: bool
    output: bool
    error: bool
    distinct_id: str
    metadata: _containers.MessageMap[str, TraceMetadataValue]
    def __init__(self, mode: _Optional[_Union[TraceMode, str]] = ..., inputs: bool = ..., output: bool = ..., error: bool = ..., distinct_id: _Optional[str] = ..., metadata: _Optional[_Mapping[str, TraceMetadataValue]] = ...) -> None: ...

class TraceMetadataValue(_message.Message):
    __slots__ = ("string_value", "int_value", "float_value", "bool_value", "remove")
    STRING_VALUE_FIELD_NUMBER: _ClassVar[int]
    INT_VALUE_FIELD_NUMBER: _ClassVar[int]
    FLOAT_VALUE_FIELD_NUMBER: _ClassVar[int]
    BOOL_VALUE_FIELD_NUMBER: _ClassVar[int]
    REMOVE_FIELD_NUMBER: _ClassVar[int]
    string_value: str
    int_value: int
    float_value: float
    bool_value: bool
    remove: bool
    def __init__(self, string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., remove: bool = ...) -> None: ...

class CallAck(_message.Message):
    __slots__ = ("error",)
    ERROR_FIELD_NUMBER: _ClassVar[int]
    error: str
    def __init__(self, error: _Optional[str] = ...) -> None: ...
