import json
from textwrap import indent
from typing import Any, Dict, List, Literal, Optional, Tuple, Union
import uuid
from opentelemetry.sdk.trace import ReadableSpan, Event
from opentelemetry.util.types import AttributeValue
from pydantic import BaseModel, Field
from typing_extensions import TypedDict
from .logger import logger
import colorama
from datetime import datetime, timezone


colorama.init()


def epoch_to_iso8601(epoch_nanos):
    # Convert nanoseconds to seconds
    epoch_seconds = epoch_nanos / 1e9
    # Create a datetime object from the epoch time
    dt_object = datetime.fromtimestamp(epoch_seconds, tz=timezone.utc)
    # Convert the datetime object to an ISO 8601 formatted string
    iso8601_timestamp = (
        dt_object.isoformat(timespec="milliseconds").replace("+00:00", "") + "Z"
    )
    return iso8601_timestamp


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

    def to_pretty_string(self) -> str:
        separator = "-------------------"
        pp = []
        if metadata := self.metadata:
            if isinstance(metadata.input.prompt.template, list):
                prompt = "\n".join(
                    f"{colorama.Fore.YELLOW}Role: {c['role']}\n{colorama.Fore.LIGHTMAGENTA_EX}{c['content']}{colorama.Fore.RESET}"
                    for c in metadata.input.prompt.template
                )
            else:
                prompt = metadata.input.prompt.template
            for k, v in metadata.input.prompt.template_args.items():
                prompt = prompt.replace(
                    k, colorama.Back.LIGHTBLUE_EX + v + colorama.Back.RESET
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
                total_tokens = llm_output.metadata.total_tokens
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
                    pretty = json.dumps(json.loads(output.value), indent=2)
                except:
                    pretty = output.value

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
        pp.insert(
            0,
            f"\n{colorama.Style.DIM}Event: {colorama.Style.NORMAL}{self.context.event_chain[-1].function_name}\n{separator}",
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


class PartialMetadataType(BaseModel):
    mdl_name: Optional[str] = Field(alias="model_name", default=None)
    provider: Optional[str] = None
    input: Optional[LLMEventInput] = None
    output: Optional[LLMOutputModel] = None
    error: Optional[Error] = None

    def to_full(self) -> Optional[Tuple[MetadataType, Optional[Error]]]:
        if self.mdl_name is None:
            return None
        if self.provider is None:
            return None
        if self.input is None:
            return None
        if self.output is None and self.error is None:
            return None
        return (
            LLMEventSchema(
                model_name=self.mdl_name,
                provider=self.provider,
                input=self.input,
                output=self.output,
            ),
            self.error,
        )


class PartialLogSchema(BaseModel):
    project_id: Optional[str] = None
    event_type: Optional[str] = None
    root_event_id: Optional[str] = None
    event_id: Optional[str] = None
    parent_event_id: Optional[str] = None
    context: LogSchemaContext
    io: IO
    error: Optional[Error] = None
    metadata: List[PartialMetadataType]

    def to_final(self) -> List[LogSchema]:
        if self.project_id is None:
            print("Missing project_id")
            return []
        if self.event_type is None:
            print("Missing event_type")
            return []
        if self.root_event_id is None:
            print("Missing root_event_id")
            return []
        if self.event_id is None:
            print("Missing event_id")
            return []
        if self.event_type not in ["log", "func_llm", "func_prob", "func_code"]:
            print("Invalid event_type")
            return []

        if self.event_type == "func_llm":
            if not self.metadata:
                return []
            if len(self.metadata) > 1:
                # Simulate extra events
                # For now not supported
                return []
            result = self.metadata[0].to_full()
            if result is None:
                return []
            full_meta, err = result
            return [
                LogSchema(
                    project_id=self.project_id,
                    event_type=self.event_type,  # type: ignore
                    root_event_id=self.root_event_id,
                    event_id=self.event_id,
                    parent_event_id=self.parent_event_id,
                    context=self.context,
                    io=self.io,
                    error=self.error or err,
                    metadata=full_meta,
                )
            ]

        # TODO: Support other event types
        if self.metadata:
            return []

        return [
            LogSchema(
                project_id=self.project_id,
                event_type=self.event_type,  # type: ignore
                root_event_id=self.root_event_id,
                event_id=self.event_id,
                parent_event_id=self.parent_event_id,
                context=self.context,
                io=self.io,
                error=self.error,
                metadata=None,
            )
        ]


def as_str(value: Optional[AttributeValue]) -> str:
    return str(value)


__uuid_lut: Dict[int, Dict[int, str]] = {}


def as_uuid(root_id: Optional[AttributeValue], value: Optional[AttributeValue]) -> str:
    assert root_id is not None
    assert type(root_id) == int, f"Expected int, got {type(root_id)}"

    assert value is not None
    assert type(value) == int, f"Expected int, got {type(value)}"
    return get_uuid(root_id, value)


def get_uuid(root_id: int, value: int) -> str:
    if root_id not in __uuid_lut:
        __uuid_lut[root_id] = {}
    if value not in __uuid_lut[root_id]:
        __uuid_lut[root_id][value] = str(uuid.uuid4())
    return __uuid_lut[root_id][value]


def as_list(value: Optional[AttributeValue]) -> List[str]:
    assert value is not None
    assert isinstance(
        value, (list, tuple)
    ), f"Expected list or tuple, got {type(value)}"

    return [str(v) for v in value]


def as_int(value: Optional[AttributeValue]) -> int:
    assert value is not None
    assert type(value) == int, f"Expected int, got {type(value)}"

    return value


def get_io_value(event: Event) -> Optional[IOValue]:
    attrs = event.attributes or {}
    params = []
    for key, value in attrs.items():
        if "." in key:
            continue
        params.append(key)
    if len(params) == 0:
        return None
    elif len(params) == 1:
        return IOValue(
            value=attrs[params[0]],
            type=TypeSchema(name=params[0], fields=attrs[f"{params[0]}.type"]),
        )
    else:
        return IOValue(
            value=[attrs[p] for p in params],
            type=TypeSchema(
                name="Tuple",
                fields={p: attrs[f"{p}.type"] for p in params},
            ),
        )


def fill_partial(event: Event, partial: PartialLogSchema) -> None:
    attrs = event.attributes or {}
    if event.name == "set_tags":
        for key, value in attrs.items():
            val = as_str(value)
            if val is not None and key != "__BAML_ID__":
                partial.context.tags[key] = val
        return
    elif event.name == "input":
        partial.io.input = get_io_value(event)
    elif event.name == "output":
        partial.io.output = get_io_value(event)
    elif event.name == "llm_prompt_template":
        partial.event_type = "func_llm"
        prompt: Union[List[LLMChat], str]
        if "chat_prompt" in attrs:
            prompt = [json.loads(k) for k in as_list(attrs["chat_prompt"])]
        else:
            prompt = as_str(attrs["prompt"])
        provider = as_str(attrs["provider"])
        template_args = json.loads(as_str(attrs["template_vars"]))
        partial.metadata.append(
            PartialMetadataType(
                provider=provider,
                input=LLMEventInput(
                    prompt=LLMEventInputPrompt(
                        template=prompt,
                        template_args=template_args,
                    ),
                    invocation_params={},
                ),
            )
        )
    elif event.name == "llm_request_start":
        partial.event_type = "func_llm"

        meta_partial = partial.metadata[-1] if partial.metadata else None

        if meta_partial:
            # Early out if we don't an input and output as that was provided by llm_prompt_template
            if meta_partial.input and (
                meta_partial.output is None and meta_partial.error is None
            ):
                return
        # Otherwise, create a new metadata entry

        if "chat_prompt" in attrs:
            prompt = [json.loads(k) for k in as_list(attrs["chat_prompt"])]
        else:
            prompt = as_str(attrs["prompt"])
        provider = as_str(attrs["provider"])

        partial.metadata.append(
            PartialMetadataType(
                provider=provider,
                input=LLMEventInput(
                    prompt=LLMEventInputPrompt(
                        template=prompt,
                        template_args={},
                    ),
                    invocation_params={},
                ),
            )
        )
    elif event.name == "llm_request_error":
        last_partial = partial.metadata[-1]
        if last_partial is None:
            return
        last_partial.error = Error(
            code=as_int(attrs["code"]),
            message=as_str(attrs["message"]),
            traceback=as_str(attrs["traceback"]),
        )
    elif event.name == "llm_request_args":
        last_partial = partial.metadata[-1]
        if last_partial is None or last_partial.input is None:
            return
        last_partial.input.invocation_params = {k: v for k, v in attrs.items()}
    elif event.name == "llm_request_end":
        last_partial = partial.metadata[-1]
        if last_partial is None:
            return
        last_partial.output = LLMOutputModel(
            raw_text=as_str(attrs["generated"]),
            metadata=LLMOutputModelMetadata.model_validate_json(as_str(attrs["meta"])),
        )
        last_partial.mdl_name = as_str(attrs["model_name"])
    elif event.name == "variant":
        partial.context.event_chain[-1].variant_name = as_str(attrs["name"])
    else:
        print("Event skipped", event.name)


def event_to_log(span: ReadableSpan) -> List[LogSchema]:
    # Validate that this is a BAML span
    if "baml" not in span.resource.attributes:
        return []

    process_id = as_str(span.resource.attributes["process_id"])
    baml_version = as_str(span.resource.attributes["baml.version"])
    project_id = as_str(span.resource.attributes.get("baml.project_id", None))

    root_span = None

    if span.attributes and "root_span" in span.attributes:
        root_span = as_int(span.attributes["root_span"])
    if root_span is None:
        return []

    partial = PartialLogSchema(
        project_id=project_id,
        root_event_id=get_uuid(root_span, root_span),
        event_id=get_uuid(root_span, span.context.span_id),
        event_type="func_code",
        context=LogSchemaContext(
            event_chain=[EventChain(function_name=span.name, variant_name=None)],
            hostname=str(span.resource.attributes["hostname"]),
            process_id=str(process_id),
            stage=as_str(span.resource.attributes.get("baml.stage", None)),
            latency_ms=(span.end_time or 0) - (span.start_time or 0),
            start_time=epoch_to_iso8601(span.start_time or 0),
            tags={
                "baml.version": baml_version,
            },
        ),
        io=IO(input=None, output=None),
        metadata=[],
    )
    for event in span.events:
        fill_partial(event, partial)

    return partial.to_final()
