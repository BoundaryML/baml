# BAML Python API: new system powered by bex_engine.

import atexit

from .baml_py import (
    BamlRuntime,
    Collector as _RustCollector,
    FunctionLog as _RustFunctionLog,
    FunctionResult,
    HostSpanManager,
    LLMCall,
    Timing,
    Usage,
    flush_events,
    get_version,
)
from .ctx_manager import CtxManager as BamlCtxManager
from .proto import decode_call_result, encode_call_args

# Flush buffered trace events on process exit so nothing is lost.
atexit.register(flush_events)


def _wrap_log(log):
    """Wrap a Rust FunctionLog in a Python FunctionLog."""
    return FunctionLog(log) if log is not None else None


class FunctionLog:
    """Python wrapper around the Rust FunctionLog that decodes the proto result."""

    __slots__ = ("_inner",)

    def __init__(self, inner: _RustFunctionLog):
        self._inner = inner

    @property
    def id(self):
        return self._inner.id

    @property
    def function_name(self):
        return self._inner.function_name

    @property
    def timing(self):
        return self._inner.timing

    @property
    def usage(self):
        return self._inner.usage

    @property
    def calls(self):
        return [_wrap_log(c) for c in self._inner.calls]

    @property
    def tags(self):
        return self._inner.tags

    @property
    def result(self):
        proto_bytes = self._inner.result
        if proto_bytes is None:
            return None
        return decode_call_result(proto_bytes)

    def __repr__(self):
        return repr(self._inner)


class Collector(_RustCollector):
    """Python subclass of the Rust Collector that wraps FunctionLog results."""

    @property
    def logs(self):
        return [_wrap_log(log) for log in super().logs]

    @property
    def last(self):
        return _wrap_log(super().last)

    def id(self, function_log_id):
        return _wrap_log(super().id(function_log_id))


def call_function_sync(rt, function_name, kwargs, ctx=None, collectors=None):
    """Call a BAML function synchronously via protobuf serialization."""
    args_proto = encode_call_args(kwargs)
    result_bytes = rt.call_function_sync(function_name, args_proto, ctx, collectors)
    return FunctionResult(decode_call_result(result_bytes))


async def call_function(rt, function_name, kwargs, ctx=None, collectors=None):
    """Call a BAML function asynchronously via protobuf serialization."""
    args_proto = encode_call_args(kwargs)
    result_bytes = await rt.call_function(function_name, args_proto, ctx, collectors)
    return FunctionResult(decode_call_result(result_bytes))


__all__ = [
    "BamlRuntime",
    "Collector",
    "FunctionLog",
    "FunctionResult",
    "HostSpanManager",
    "LLMCall",
    "Timing",
    "Usage",
    "BamlCtxManager",
    "flush_events",
    "get_version",
    "call_function",
    "call_function_sync",
]
