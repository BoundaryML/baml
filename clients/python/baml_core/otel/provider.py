import contextvars
from itertools import chain
import platform
import typing
import uuid
from opentelemetry import trace
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

from ..cache_manager import CacheManager
import typeguard
from baml_core.logger import logger


from .helper import event_to_log, try_serialize
from baml_version import __version__
from ..services.api import APIWrapper

from ..services.api_types import LogSchema
import logging


@typing.final
class CustomBackendExporter(SpanExporter):
    __project_id: typing.Optional[str]
    __message_override_callback: typing.Dict[int, typing.Callable[[LogSchema], None]]
    __print_log_level: int

    def __init__(self) -> None:
        super().__init__()
        self.__api_wrapper: typing.Optional[APIWrapper] = None
        self.__process_id = str(uuid.uuid4())
        self.__project_id = None
        self.__message_override_callback = {}
        self.__print_log_level = logging.WARNING

    def set_print_log_level(self, level: int) -> None:
        self.__print_log_level = level

    def set_gloo_api(self, api_wrapper: typing.Optional[APIWrapper]) -> None:
        if api_wrapper:
            self.__api_wrapper = api_wrapper
            self.__project_id = api_wrapper.project_id
            self.__process_id = api_wrapper.session_id
        else:
            self.__api_wrapper = None
            self.__process_id = str(uuid.uuid4())
            self.__project_id = None

    def remove_message_override_callback(self, _id: int) -> None:
        if _id in self.__message_override_callback:
            self.__message_override_callback.pop(_id)

    def add_message_override_callback(
        self, callback: typing.Callable[[LogSchema], None]
    ) -> int:
        _id = uuid.uuid4().int
        self.__message_override_callback[_id] = callback
        return _id

    def export(self, spans: typing.Sequence[ReadableSpan]) -> SpanExportResult:
        # Convert spans to your backend's desired format
        # and send them. This is a simple example that just
        # prints the span names. You should replace this with
        # the logic to send the spans to your backend.
        items = list(
            chain.from_iterable(
                event_to_log(
                    span, project_id=self.__project_id, process_id=self.__process_id
                )
                for span in spans
            )
        )

        for item in items:
            item.print(self.__print_log_level)
            if self.__message_override_callback:
                # Run every callback in parallel
                for cb in self.__message_override_callback.values():
                    try:
                        # if the msg overides fail, export will also fail.
                        # note, this may mutate the item
                        cb(item)
                    except Exception as e:
                        # Swallow for now...
                        logger.error(
                            "Failed to override and export messages. Will still emit to Dashboard",
                            e,
                        )

            CacheManager.save_llm_request(item)

            if self.__api_wrapper:
                # TODO: Send a single large payload.
                # send them to the backend.
                # This function can't fail.
                self.__api_wrapper.log_sync(payload=item)

        # If the export was successful, return
        # SpanExportResult.SUCCESS, otherwise, return
        # SpanExportResult.FAILURE
        return SpanExportResult.SUCCESS

    # def shutdown(self) -> None:
    #     # Any cleanup logic for your exporter goes here
    #     pass

    # def force_flush(self, timeout_millis: int = 30000) -> bool:
    #     return self.


attributes_context: contextvars.ContextVar[
    typing.Dict[int, typing.Dict[str, types.AttributeValue]]
] = contextvars.ContextVar("attributes", default={})

# Context variable to store the root span
parent_history = contextvars.ContextVar[
    typing.Dict[int, typing.List[typing.Tuple[int, str]]]
]("parent_history", default={})


# We can't use events for tags because we need to do some magic for child
# spans. Instead, we use a contextvar to store the tags for the current span.
# We set the tags on the span when it's complete.
# This is exposed publicly, so we typecheck it.
@typeguard.typechecked
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


class BamlSpanContextManager:
    def __init__(
        self,
        name: str,
        parent_id: int,
        span: Span,
        kwargs: typing.Dict[str, typing.Any],
    ):
        self.parent_id = parent_id
        self.span = span
        self.name = name

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
        span_id = self.span.get_span_context().span_id

        if self.parent_id == 0:
            current_history = parent_history.get()
            current_history[span_id] = [(span_id, self.name)]
            parent_history.set(current_history)
        else:
            current_history = parent_history.get()
            current_history[span_id] = current_history.get(self.parent_id, []) + [
                (span_id, self.name)
            ]
            parent_history.set(current_history)

        current_attributes = attributes_context.get()
        span_attributes = current_attributes.get(
            self.parent_id, {"__BAML_ID__": span_id}
        )
        current_attributes[span_id] = span_attributes
        attributes_context.set(current_attributes)
        return self

    def complete(self, result: typing.Any) -> None:
        if result is not None:
            result, type_name = try_serialize(result)
            self.span.add_event("output", {"result": result, "result.type": type_name})
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

        current_history = parent_history.get()
        span_history = current_history.pop(span_id, None)
        parent_history.set(current_history)

        if span_history is not None:
            self.span.set_attribute(
                "root_span_ids", list(map(lambda m: m[0], span_history))
            )
            self.span.set_attribute(
                "root_span_names", list(map(lambda m: m[1], span_history))
            )


# Initialize to the default No-op tracer.

# Set up your TracerProvider with the custom settings

__provider = TracerProvider(
    resource=Resource.create(
        {
            "baml": "baml",
            "baml.version": __version__,
            "hostname": platform.node(),
        }
    )
)
baml_tracer = __provider.get_tracer("BAML_TRACING")
__exporter = CustomBackendExporter()
__processor = BatchSpanProcessor(__exporter, max_export_batch_size=5)
__provider.add_span_processor(__processor)


def flush_trace_logs() -> None:
    __provider.force_flush(10000)
    __processor.force_flush(10000)


def use_tracing(api: typing.Optional[APIWrapper] = None) -> None:
    __exporter.set_gloo_api(api)


def add_message_transformer_hook(callback: typing.Callable[[LogSchema], None]) -> int:
    return __exporter.add_message_override_callback(callback)


def remove_message_transformer_hook(_id: int) -> None:
    __exporter.remove_message_override_callback(_id)


def set_print_log_level(level: int) -> None:
    __exporter.set_print_log_level(level)
