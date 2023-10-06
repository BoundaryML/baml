from __future__ import annotations
from typing import Any, Dict, List, Mapping, Optional, Union
from typing_extensions import TypedDict, Literal


from pydantic import BaseModel, Field
from enum import Enum


class TypeSchema(BaseModel):
    name: str
    fields: Any


class IOValue(BaseModel):
    value: Any
    type: TypeSchema


class IO(BaseModel):
    input: Optional[IOValue]
    output: Optional[IOValue]


class LLMOutputModelMetadata(BaseModel):
    logprobs: Optional[Any]
    prompt_tokens: Optional[int]
    output_tokens: Optional[int]
    total_tokens: Optional[int]


class LLMOutputModel(BaseModel):
    raw_text: str
    metadata: LLMOutputModelMetadata


class LLMChat(TypedDict):
    role: Literal["assistant", "user", "system"]
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


class EventChain(BaseModel):
    function_name: str
    variant_name: Optional[str]


class LogSchemaContext(BaseModel):
    hostname: str
    process_id: str
    stage: Optional[str]
    latency_ms: Optional[int]
    start_time: str
    tags: Dict[str, str]
    event_chain: List[EventChain]


class Error(BaseModel):
    code: int
    message: str
    traceback: Optional[str]


MetadataType = LLMEventSchema


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


## Process management
class StartProcessRequest(BaseModel):
    project_id: str
    session_id: str
    stage: str
    hostname: str
    start_time: str
    tags: Mapping[str, str]


class EndProcessRequest(BaseModel):
    project_id: str
    session_id: str
    end_time: str


### Tests
class CreateCycleRequest(BaseModel):
    project_id: str
    session_id: str


class CreateCycleResponse(BaseModel):
    test_cycle_id: str
    dashboard_url: str


class LogTestTags(BaseModel):
    test_cycle_id: str
    test_dataset_name: str
    test_case_name: str
    test_case_arg_name: str


class TestCaseStatus(str, Enum):
    QUEUED = "QUEUED"
    RUNNING = "RUNNING"
    PASSED = "PASSED"
    FAILED = "FAILED"
    CANCELLED = "CANCELLED"
    EXPECTED_FAILURE = "EXPECTED_FAILURE"


class CreateTestCase(BaseModel):
    project_id: str = ""
    test_cycle_id: str = ""
    test_dataset_name: str
    test_name: str
    test_case_args: List[Dict[str, str]]


class UpdateTestCase(BaseModel):
    project_id: str = ""
    test_cycle_id: str = ""
    test_dataset_name: str
    test_case_definition_name: str
    test_case_arg_name: str
    status: TestCaseStatus
    error_data: Optional[Any]


class CacheRequest(BaseModel):
    project_id: str = ""
    provider: str
    prompt: Union[str, List[LLMChat]]
    prompt_vars: Dict[str, str]
    invocation_params: Dict[str, Any]


class CacheResponse(BaseModel):
    mdl_name: str = Field(alias="model_name")
    llm_output: LLMOutputModel
    latency_ms: int
