from __future__ import annotations
import json
from typing import Any, Dict, List, Mapping, Optional, Union
from typing_extensions import TypedDict, Literal
from ..logger import logger

from pydantic import BaseModel, Field
from enum import Enum


try:
    import colorama

    colorama.init()
except ImportError:

    class MockAnsi:
        BLACK = ""
        RED = ""
        GREEN = ""
        YELLOW = ""
        BLUE = ""
        MAGENTA = ""
        CYAN = ""
        WHITE = ""
        RESET = ""
        LIGHTBLACK_EX = ""
        LIGHTRED_EX = ""
        LIGHTGREEN_EX = ""
        LIGHTYELLOW_EX = ""
        LIGHTBLUE_EX = ""
        LIGHTMAGENTA_EX = ""
        LIGHTCYAN_EX = ""
        LIGHTWHITE_EX = ""

    class MockStyle:
        DIM = ""
        NORMAL = ""
        BRIGHT = ""
        RESET_ALL = ""

    class MockColorama:
        Fore = MockAnsi()
        Back = MockAnsi()
        Style = MockStyle()

    colorama = MockColorama()  # type: ignore


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
    finish_reason: Optional[str] = None


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
    latency_ms: int
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

    def to_pretty_string(self) -> str:
        separator = "-------------------"
        pp = []

        if metadata := self.metadata:
            if isinstance(metadata.input.prompt.template, list):
                prompt = "\n".join(
                    f"{colorama.Fore.YELLOW}Role: {c['role']}\n{  colorama.Fore.LIGHTMAGENTA_EX}{c['content']}{colorama.Fore.RESET }"
                    for c in metadata.input.prompt.template
                )
            else:
                prompt = metadata.input.prompt.template
            for k, v in metadata.input.prompt.template_args.items():
                prompt = prompt.replace(
                    k, colorama.Style.BRIGHT + v + colorama.Style.NORMAL
                )
            pp.extend(
                [
                    colorama.Style.DIM + "Prompt" + colorama.Style.NORMAL,
                    prompt,
                    separator,
                ]
            )

            # This is an LLM Event
            if llm_output := metadata.output:
                prompt_tokens = llm_output.metadata.prompt_tokens
                output_tokens = llm_output.metadata.output_tokens
                _total_tokens = llm_output.metadata.total_tokens
                pp.append(
                    colorama.Style.DIM
                    + f"Raw LLM Output (Tokens: prompt={prompt_tokens} output={output_tokens})"
                    + colorama.Style.NORMAL
                )
                pp.append(
                    colorama.Style.DIM + llm_output.raw_text + colorama.Style.NORMAL
                )
                pp.append(separator)
            if output := self.io.output:
                pp.append(
                    colorama.Style.DIM
                    + "Deserialized Output "
                    + f"({colorama.Fore.LIGHTBLUE_EX}{output.type}{colorama.Fore.RESET}):"
                    + colorama.Style.NORMAL
                )
                try:
                    # TODO: Figure out why we get a tuple here sometimes
                    if isinstance(output.value, tuple) and len(output.value) == 1:
                        pretty = json.dumps(json.loads(output.value[0]), indent=2)
                    pretty = json.dumps(json.loads(output.value), indent=2)
                except Exception:
                    pretty = str(output.value)

                pp.append(colorama.Fore.LIGHTBLUE_EX + pretty + colorama.Fore.RESET)
                pp.append(separator)
        if error := self.error:
            pp.append("Error")
            pp.append(
                colorama.Style.BRIGHT + str(error.message) + colorama.Style.NORMAL
            )
            pp.append(separator)
        if len(pp) == 0:
            return ""
        cached_string = ""
        if "__cached" in self.context.tags:
            cached_string = f"{colorama.Fore.LIGHTYELLOW_EX} Cache Hit! Saved {self.context.tags['__cached_latency_ms']}ms {colorama.Fore.RESET} "

        pp.insert(
            0,
            f"\n{cached_string}{colorama.Style.DIM}Event: {colorama.Style.NORMAL}{self.context.event_chain[-1].function_name}\n{separator}",
        )
        if pp[-1] == separator:
            pp[-1] = "-" * 80
        return "\n".join(pp)

    def print(self) -> None:
        if log := self.to_pretty_string():
            if self.error:
                logger.error(log)
            else:
                logger.info(log)


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
    provider: str
    prompt: Union[str, List[LLMChat]]
    prompt_vars: Dict[str, str]
    invocation_params: Dict[str, Any]


class CacheResponse(BaseModel):
    mdl_name: str = Field(alias="model_name")
    llm_output: LLMOutputModel
    latency_ms: int
