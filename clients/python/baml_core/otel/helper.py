from typing import Any, Dict, List, Literal, Optional, Union
from opentelemetry.sdk.trace import ReadableSpan
from pydantic import BaseModel, Field
from typing_extensions import TypedDict


class Error(BaseModel):
    code: int
    message: str
    traceback: Optional[str]


class EventChain(BaseModel):
    function_name: str
    variant_name: Optional[str]


class LLMOutputModelMetadata(BaseModel):
    logprobs: Optional[Any]
    prompt_tokens: Optional[int]
    output_tokens: Optional[int]
    total_tokens: Optional[int]


class LLMOutputModel(BaseModel):
    raw_text: str
    metadata: LLMOutputModelMetadata


class LLMChat(TypedDict):
    role: Union[Literal["assistant", "user", "system"], str]
    content: str


class LLMEventInputPrompt(BaseModel):
    template: Union[str, List[LLMChat]]
    template_args: Dict[str, str]


class LLMEventInput(BaseModel):
    prompt: LLMEventInputPrompt
    invocation_params: Dict[str, Any]


class LLMEventSchema(BaseModel):
    mdl_name: str = Field(alias="model_name")
    provider: str
    input: LLMEventInput
    output: Optional[LLMOutputModel]


MetadataType = LLMEventSchema


class LogSchemaContext(BaseModel):
    hostname: str
    process_id: str
    stage: Optional[str]
    latency_ms: Optional[int]
    start_time: str
    tags: Dict[str, str]
    event_chain: List[EventChain]


class TypeSchema(BaseModel):
    name: str
    fields: Any


class IOValue(BaseModel):
    value: Any
    type: TypeSchema


class IO(BaseModel):
    input: Optional[IOValue]
    output: Optional[IOValue]


class LogSchema(BaseModel):
    project_id: str
    event_type: Literal["log", "func_llm", "func_prob", "func_code"]
    root_event_id: str
    event_id: str
    parent_event_id: Optional[str]
    context: LogSchemaContext
    io: IO
    error: Optional[Error]
    metadata: Optional[MetadataType]


def event_to_log(span: ReadableSpan) -> None:
    print("----")
    print(span.name, span.context.span_id)
    if span.attributes:
        print("Attributes", list(span.attributes.items()))
    for e in span.events:
        print("Event", e.name, e.attributes)
    print(span.resource)
