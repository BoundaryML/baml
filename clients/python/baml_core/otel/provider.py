import contextvars
import json
import typing
from opentelemetry import trace, context
from opentelemetry.trace.span import Span
from opentelemetry.util import types
from opentelemetry.sdk.trace.export import (
    SpanExporter,
    SpanExportResult,
    BatchSpanProcessor,
)
from opentelemetry.trace import get_current_span
from opentelemetry.sdk.trace import ReadableSpan, TracerProvider
from opentelemetry.sdk.resources import Resource
from pydantic import BaseModel
from .. import __version__


@typing.final
class CustomBackendExporter(SpanExporter):
    def __init__(self) -> None:
        pass

    def export(self, spans: typing.Sequence[ReadableSpan]) -> SpanExportResult:
        # Convert spans to your backend's desired format
        # and send them. This is a simple example that just
        # prints the span names. You should replace this with
        # the logic to send the spans to your backend.
        for span in spans:
            parent = span.parent
            print(span.name, span.context.span_id, parent.span_id if parent else None)
            print(span.attributes)

        # If the export was successful, return
        # SpanExportResult.SUCCESS, otherwise, return
        # SpanExportResult.FAILURE
        return SpanExportResult.SUCCESS

    def shutdown(self) -> None:
        # Any cleanup logic for your exporter goes here
        pass


from opentelemetry.sdk.trace import Tracer


attributes_context: contextvars.ContextVar[
    typing.Dict[int, typing.Dict[str, types.AttributeValue]]
] = contextvars.ContextVar("attributes", default={})
# Context variable to store the root span
root_span = contextvars.ContextVar[typing.Optional[int]]("root_span", default=None)


# We can't use events for tags because we need to do some magic for child
# spans. Instead, we use a contextvar to store the tags for the current span.
# We set the tags on the span when it's complete.
def set_tags(**attributes: typing.Optional[types.AttributeValue]) -> None:
    span: typing.Optional[Span] = get_current_span()
    if span:
        span_id = span.get_span_context().span_id
        current_attributes = attributes_context.get()

        span_attributes = current_attributes.get(span_id, {})
        # IF we may be in a nested context, we need to make sure
        # that the span ID is set correctly.
        if span_attributes.get("__BAML_ID__") != span_id:
            span_attributes = span_attributes.copy()
            span_attributes["__BAML_ID__"] = span_id

        # If an attribute's value is None, unset it (remove it).
        # Otherwise, if it's a string, update or set it.
        for key, value in attributes.items():
            if value is None:
                span_attributes.pop(key, None)
            elif isinstance(value, str):
                span_attributes[key] = value
        current_attributes[span_id] = span_attributes
        attributes_context.set(current_attributes)


def create_event(name: str, attributes: typing.Dict[str, types.AttributeValue]) -> None:
    span: typing.Optional[Span] = get_current_span()
    if span:
        span.add_event(name, attributes)


def try_serialize_inner(
    value: typing.Any,
) -> typing.Union[str, int, float, bool]:
    if value is None:
        return ""
    if isinstance(value, (str, int, float, bool)):
        return value
    if isinstance(value, BaseModel):
        return value.model_dump_json()
    try:
        return json.dumps(value, default=str)
    except BaseException:
        return "<unserializable value>"


def try_serialize(value: typing.Any) -> typing.Tuple[types.AttributeValue, str]:
    if value is None:
        return "", "None"
    if isinstance(value, (str, int, float, bool)):
        return value, type(value).__name__
    if isinstance(value, list):
        if len(value) == 0:
            return value, "List[]"
        same_type = list(set(type(item) for item in value))
        if len(same_type) == 1:
            type_name = same_type[0].__name__
            if type(value[0]) in (str, int, float, bool):
                return (value, f"List[{type_name}]")
            return (
                typing.cast(
                    types.AttributeValue, [try_serialize_inner(v) for v in value]
                ),
                f"List[{type_name}]",
            )

    if isinstance(value, tuple) and all(
        isinstance(item, (str, int, float, bool)) for item in value
    ):
        return (value, f"Tuple[{type(value[0]).__name__}]")
    if isinstance(value, BaseModel):
        return value.model_dump_json(), type(value).__name__
    try:
        return json.dumps(value, default=str), type(value).__name__
    except BaseException:
        return "<unserializable value>", type(value).__name__


class BamlSpanContextManager:
    def __init__(
        self, parent_id: int, span: Span, kwargs: typing.Dict[str, typing.Any]
    ):
        self.parent_id = parent_id
        self.span = span

        if "self" in kwargs:
            kwargs.pop("self")

        if "cls" in kwargs:
            kwargs.pop("cls")
        attributes = {}
        for key, value in kwargs.items():
            value, type_name = try_serialize(value)
            attributes.update({key: value, f"{key}.type": type_name})
        span.add_event("input", attributes)

    def __enter__(self) -> "BamlSpanContextManager":
        if self.parent_id == 0:
            root_span.set(self.span.get_span_context().span_id)
        span_id = self.span.get_span_context().span_id
        current_attributes = attributes_context.get()
        span_attributes = current_attributes.get(
            self.parent_id, {"__BAML_ID__": span_id}
        )
        current_attributes[span_id] = span_attributes
        attributes_context.set(current_attributes)
        return self

    def complete(self, result: typing.Any) -> None:
        if result is not None:
            self.span.add_event("output", {"result": result})
        self.span.set_status(trace.Status(trace.StatusCode.OK))

    def __exit__(
        self, exc_type: typing.Any, exc_val: typing.Any, exc_tb: typing.Any
    ) -> None:
        span_id = self.span.get_span_context().span_id
        current_attributes = attributes_context.get()
        attributes = current_attributes.pop(span_id, None)
        attributes_context.set(current_attributes)
        if attributes:
            self.span.add_event("set_tags", attributes)
        root_span_id = root_span.get()
        if root_span_id is not None:
            self.span.set_attribute("root_span", root_span_id)
        if self.parent_id == 0:
            root_span.set(None)


# Initialize to the default No-op tracer.

# Set up your TracerProvider with the custom settings
provider = TracerProvider(
    resource=Resource.create(
        {
            "service.name": "baml",
            "service.version": __version__,
        }
    )
)
baml_tracer = provider.get_tracer("BAML_TRACING")


def use_tracing() -> None:
    global provider
    provider.add_span_processor(
        BatchSpanProcessor(CustomBackendExporter(), max_export_batch_size=10)
    )
