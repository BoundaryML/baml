from baml_core.cffi.v1 import baml_outbound_pb2 as _baml_outbound_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class RuntimeEvent(_message.Message):
    __slots__ = ("span_id", "parent_span_id", "root_span_id", "timestamp_ms", "call_stack", "event", "call_id")
    SPAN_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_SPAN_ID_FIELD_NUMBER: _ClassVar[int]
    ROOT_SPAN_ID_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_MS_FIELD_NUMBER: _ClassVar[int]
    CALL_STACK_FIELD_NUMBER: _ClassVar[int]
    EVENT_FIELD_NUMBER: _ClassVar[int]
    CALL_ID_FIELD_NUMBER: _ClassVar[int]
    span_id: str
    parent_span_id: str
    root_span_id: str
    timestamp_ms: int
    call_stack: _containers.RepeatedScalarFieldContainer[str]
    event: EventKind
    call_id: int
    def __init__(self, span_id: _Optional[str] = ..., parent_span_id: _Optional[str] = ..., root_span_id: _Optional[str] = ..., timestamp_ms: _Optional[int] = ..., call_stack: _Optional[_Iterable[str]] = ..., event: _Optional[_Union[EventKind, _Mapping]] = ..., call_id: _Optional[int] = ...) -> None: ...

class EventKind(_message.Message):
    __slots__ = ("function_start", "function_end", "set_tags", "log", "custom")
    FUNCTION_START_FIELD_NUMBER: _ClassVar[int]
    FUNCTION_END_FIELD_NUMBER: _ClassVar[int]
    SET_TAGS_FIELD_NUMBER: _ClassVar[int]
    LOG_FIELD_NUMBER: _ClassVar[int]
    CUSTOM_FIELD_NUMBER: _ClassVar[int]
    function_start: FunctionStartEvent
    function_end: FunctionEndEvent
    set_tags: SetTagsEvent
    log: LogEvent
    custom: CustomEvent
    def __init__(self, function_start: _Optional[_Union[FunctionStartEvent, _Mapping]] = ..., function_end: _Optional[_Union[FunctionEndEvent, _Mapping]] = ..., set_tags: _Optional[_Union[SetTagsEvent, _Mapping]] = ..., log: _Optional[_Union[LogEvent, _Mapping]] = ..., custom: _Optional[_Union[CustomEvent, _Mapping]] = ...) -> None: ...

class FunctionStartEvent(_message.Message):
    __slots__ = ("name", "args", "tags")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ARGS_FIELD_NUMBER: _ClassVar[int]
    TAGS_FIELD_NUMBER: _ClassVar[int]
    name: str
    args: _containers.RepeatedCompositeFieldContainer[_baml_outbound_pb2.BamlOutboundValue]
    tags: _containers.RepeatedCompositeFieldContainer[TagEntry]
    def __init__(self, name: _Optional[str] = ..., args: _Optional[_Iterable[_Union[_baml_outbound_pb2.BamlOutboundValue, _Mapping]]] = ..., tags: _Optional[_Iterable[_Union[TagEntry, _Mapping]]] = ...) -> None: ...

class FunctionEndEvent(_message.Message):
    __slots__ = ("name", "result", "duration_ms", "error")
    NAME_FIELD_NUMBER: _ClassVar[int]
    RESULT_FIELD_NUMBER: _ClassVar[int]
    DURATION_MS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    name: str
    result: _baml_outbound_pb2.BamlOutboundValue
    duration_ms: int
    error: str
    def __init__(self, name: _Optional[str] = ..., result: _Optional[_Union[_baml_outbound_pb2.BamlOutboundValue, _Mapping]] = ..., duration_ms: _Optional[int] = ..., error: _Optional[str] = ...) -> None: ...

class SetTagsEvent(_message.Message):
    __slots__ = ("tags",)
    TAGS_FIELD_NUMBER: _ClassVar[int]
    tags: _containers.RepeatedCompositeFieldContainer[TagEntry]
    def __init__(self, tags: _Optional[_Iterable[_Union[TagEntry, _Mapping]]] = ...) -> None: ...

class SourceLocation(_message.Message):
    __slots__ = ("file_id", "line", "column", "start_offset", "end_offset")
    FILE_ID_FIELD_NUMBER: _ClassVar[int]
    LINE_FIELD_NUMBER: _ClassVar[int]
    COLUMN_FIELD_NUMBER: _ClassVar[int]
    START_OFFSET_FIELD_NUMBER: _ClassVar[int]
    END_OFFSET_FIELD_NUMBER: _ClassVar[int]
    file_id: int
    line: int
    column: int
    start_offset: int
    end_offset: int
    def __init__(self, file_id: _Optional[int] = ..., line: _Optional[int] = ..., column: _Optional[int] = ..., start_offset: _Optional[int] = ..., end_offset: _Optional[int] = ...) -> None: ...

class LogEvent(_message.Message):
    __slots__ = ("level", "data", "source")
    LEVEL_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    level: str
    data: _baml_outbound_pb2.BamlOutboundValue
    source: SourceLocation
    def __init__(self, level: _Optional[str] = ..., data: _Optional[_Union[_baml_outbound_pb2.BamlOutboundValue, _Mapping]] = ..., source: _Optional[_Union[SourceLocation, _Mapping]] = ...) -> None: ...

class CustomEvent(_message.Message):
    __slots__ = ("name", "data")
    NAME_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    name: str
    data: _baml_outbound_pb2.BamlOutboundValue
    def __init__(self, name: _Optional[str] = ..., data: _Optional[_Union[_baml_outbound_pb2.BamlOutboundValue, _Mapping]] = ...) -> None: ...

class TagEntry(_message.Message):
    __slots__ = ("key", "value")
    KEY_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    key: str
    value: str
    def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
