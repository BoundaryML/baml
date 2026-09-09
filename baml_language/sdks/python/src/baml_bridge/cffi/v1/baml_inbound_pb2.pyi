from baml_bridge.cffi.v1 import baml_handle_pb2 as _baml_handle_pb2
from baml_bridge.cffi.v1 import baml_type_pb2 as _baml_type_pb2
from baml_bridge.cffi.v1 import baml_outbound_pb2 as _baml_outbound_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class InboundValue(_message.Message):
    __slots__ = ("value_type", "string_value", "int_value", "float_value", "bool_value", "list_value", "map_value", "class_value", "enum_value", "handle", "uint8array_value", "bigint_value", "ty_value", "ty_def_value", "media_value", "prompt_ast_value")
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
    def __init__(self, value_type: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., string_value: _Optional[str] = ..., int_value: _Optional[int] = ..., float_value: _Optional[float] = ..., bool_value: bool = ..., list_value: _Optional[_Union[InboundListValue, _Mapping]] = ..., map_value: _Optional[_Union[InboundMapValue, _Mapping]] = ..., class_value: _Optional[_Union[InboundClassValue, _Mapping]] = ..., enum_value: _Optional[_Union[InboundEnumValue, _Mapping]] = ..., handle: _Optional[_Union[_baml_handle_pb2.BamlHandle, _Mapping]] = ..., uint8array_value: _Optional[bytes] = ..., bigint_value: _Optional[str] = ..., ty_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., ty_def_value: _Optional[_Union[_baml_type_pb2.BamlTyDef, _Mapping]] = ..., media_value: _Optional[_Union[_baml_outbound_pb2.BamlValueMedia, _Mapping]] = ..., prompt_ast_value: _Optional[_Union[_baml_outbound_pb2.BamlValuePromptAst, _Mapping]] = ...) -> None: ...

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
    __slots__ = ("type_var", "type_value", "type_definition", "type_reference")
    TYPE_VAR_FIELD_NUMBER: _ClassVar[int]
    TYPE_VALUE_FIELD_NUMBER: _ClassVar[int]
    TYPE_DEFINITION_FIELD_NUMBER: _ClassVar[int]
    TYPE_REFERENCE_FIELD_NUMBER: _ClassVar[int]
    type_var: str
    type_value: _baml_type_pb2.BamlTy
    type_definition: _baml_type_pb2.BamlTyDef
    type_reference: int
    def __init__(self, type_var: _Optional[str] = ..., type_value: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., type_definition: _Optional[_Union[_baml_type_pb2.BamlTyDef, _Mapping]] = ..., type_reference: _Optional[int] = ...) -> None: ...

class InterfaceMethodTarget(_message.Message):
    __slots__ = ("view", "member", "type_args")
    VIEW_FIELD_NUMBER: _ClassVar[int]
    MEMBER_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    view: int
    member: str
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    def __init__(self, view: _Optional[int] = ..., member: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ...) -> None: ...

class ConcreteMethodTarget(_message.Message):
    __slots__ = ("receiver", "class_name", "interface_pattern", "inherent", "member", "type_args")
    RECEIVER_FIELD_NUMBER: _ClassVar[int]
    CLASS_NAME_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_PATTERN_FIELD_NUMBER: _ClassVar[int]
    INHERENT_FIELD_NUMBER: _ClassVar[int]
    MEMBER_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    receiver: int
    class_name: str
    interface_pattern: _baml_type_pb2.BamlTy
    inherent: bool
    member: str
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    def __init__(self, receiver: _Optional[int] = ..., class_name: _Optional[str] = ..., interface_pattern: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., inherent: bool = ..., member: _Optional[str] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ...) -> None: ...

class CallFunctionArgs(_message.Message):
    __slots__ = ("kwargs", "call_id", "type_args", "function_name", "function_handle", "interface_method", "concrete_method")
    KWARGS_FIELD_NUMBER: _ClassVar[int]
    CALL_ID_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_NAME_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_HANDLE_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_METHOD_FIELD_NUMBER: _ClassVar[int]
    CONCRETE_METHOD_FIELD_NUMBER: _ClassVar[int]
    kwargs: _containers.RepeatedCompositeFieldContainer[InboundMapEntry]
    call_id: int
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    function_name: str
    function_handle: int
    interface_method: InterfaceMethodTarget
    concrete_method: ConcreteMethodTarget
    def __init__(self, kwargs: _Optional[_Iterable[_Union[InboundMapEntry, _Mapping]]] = ..., call_id: _Optional[int] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ..., function_name: _Optional[str] = ..., function_handle: _Optional[int] = ..., interface_method: _Optional[_Union[InterfaceMethodTarget, _Mapping]] = ..., concrete_method: _Optional[_Union[ConcreteMethodTarget, _Mapping]] = ...) -> None: ...

class CallAck(_message.Message):
    __slots__ = ("error",)
    ERROR_FIELD_NUMBER: _ClassVar[int]
    error: str
    def __init__(self, error: _Optional[str] = ...) -> None: ...

class RegisterHostAdapterRequest(_message.Message):
    __slots__ = ("name", "implementations", "type_args")
    NAME_FIELD_NUMBER: _ClassVar[int]
    IMPLEMENTATIONS_FIELD_NUMBER: _ClassVar[int]
    TYPE_ARGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    implementations: _containers.RepeatedCompositeFieldContainer[HostAdapterImplementation]
    type_args: _containers.RepeatedCompositeFieldContainer[BamlTyArg]
    def __init__(self, name: _Optional[str] = ..., implementations: _Optional[_Iterable[_Union[HostAdapterImplementation, _Mapping]]] = ..., type_args: _Optional[_Iterable[_Union[BamlTyArg, _Mapping]]] = ...) -> None: ...

class HostAdapterImplementation(_message.Message):
    __slots__ = ("interface_template", "methods")
    INTERFACE_TEMPLATE_FIELD_NUMBER: _ClassVar[int]
    METHODS_FIELD_NUMBER: _ClassVar[int]
    interface_template: _baml_type_pb2.BamlTy
    methods: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, interface_template: _Optional[_Union[_baml_type_pb2.BamlTy, _Mapping]] = ..., methods: _Optional[_Iterable[str]] = ...) -> None: ...

class RegisteredHostAdapter(_message.Message):
    __slots__ = ("adapter_type", "class_type", "callbacks", "interface_types")
    ADAPTER_TYPE_FIELD_NUMBER: _ClassVar[int]
    CLASS_TYPE_FIELD_NUMBER: _ClassVar[int]
    CALLBACKS_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_TYPES_FIELD_NUMBER: _ClassVar[int]
    adapter_type: _baml_outbound_pb2.BamlOutboundHandle
    class_type: _baml_outbound_pb2.BamlOutboundHandle
    callbacks: _containers.RepeatedCompositeFieldContainer[HostAdapterCallbackSlot]
    interface_types: _containers.RepeatedCompositeFieldContainer[_baml_outbound_pb2.BamlOutboundHandle]
    def __init__(self, adapter_type: _Optional[_Union[_baml_outbound_pb2.BamlOutboundHandle, _Mapping]] = ..., class_type: _Optional[_Union[_baml_outbound_pb2.BamlOutboundHandle, _Mapping]] = ..., callbacks: _Optional[_Iterable[_Union[HostAdapterCallbackSlot, _Mapping]]] = ..., interface_types: _Optional[_Iterable[_Union[_baml_outbound_pb2.BamlOutboundHandle, _Mapping]]] = ...) -> None: ...

class HostAdapterCallbackSlot(_message.Message):
    __slots__ = ("implementation_index", "method")
    IMPLEMENTATION_INDEX_FIELD_NUMBER: _ClassVar[int]
    METHOD_FIELD_NUMBER: _ClassVar[int]
    implementation_index: int
    method: str
    def __init__(self, implementation_index: _Optional[int] = ..., method: _Optional[str] = ...) -> None: ...

class CreateHostAdapterRequest(_message.Message):
    __slots__ = ("adapter_type", "receiver", "callbacks")
    ADAPTER_TYPE_FIELD_NUMBER: _ClassVar[int]
    RECEIVER_FIELD_NUMBER: _ClassVar[int]
    CALLBACKS_FIELD_NUMBER: _ClassVar[int]
    adapter_type: int
    receiver: InboundValue
    callbacks: _containers.RepeatedCompositeFieldContainer[InboundValue]
    def __init__(self, adapter_type: _Optional[int] = ..., receiver: _Optional[_Union[InboundValue, _Mapping]] = ..., callbacks: _Optional[_Iterable[_Union[InboundValue, _Mapping]]] = ...) -> None: ...

class ProjectInterfaceRequest(_message.Message):
    __slots__ = ("receiver", "interface_type")
    RECEIVER_FIELD_NUMBER: _ClassVar[int]
    INTERFACE_TYPE_FIELD_NUMBER: _ClassVar[int]
    receiver: int
    interface_type: BamlTyArg
    def __init__(self, receiver: _Optional[int] = ..., interface_type: _Optional[_Union[BamlTyArg, _Mapping]] = ...) -> None: ...

class HostOperationRequest(_message.Message):
    __slots__ = ("register", "create", "project")
    REGISTER_FIELD_NUMBER: _ClassVar[int]
    CREATE_FIELD_NUMBER: _ClassVar[int]
    PROJECT_FIELD_NUMBER: _ClassVar[int]
    register: RegisterHostAdapterRequest
    create: CreateHostAdapterRequest
    project: ProjectInterfaceRequest
    def __init__(self, register: _Optional[_Union[RegisterHostAdapterRequest, _Mapping]] = ..., create: _Optional[_Union[CreateHostAdapterRequest, _Mapping]] = ..., project: _Optional[_Union[ProjectInterfaceRequest, _Mapping]] = ...) -> None: ...

class HostOperationResult(_message.Message):
    __slots__ = ("registered", "value", "failure")
    REGISTERED_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    FAILURE_FIELD_NUMBER: _ClassVar[int]
    registered: RegisteredHostAdapter
    value: _baml_outbound_pb2.BamlOutboundValue
    failure: _baml_outbound_pb2.BamlOutboundResult
    def __init__(self, registered: _Optional[_Union[RegisteredHostAdapter, _Mapping]] = ..., value: _Optional[_Union[_baml_outbound_pb2.BamlOutboundValue, _Mapping]] = ..., failure: _Optional[_Union[_baml_outbound_pb2.BamlOutboundResult, _Mapping]] = ...) -> None: ...
