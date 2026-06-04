# BAML Python runtime surface — the `baml_core` package.
#
# Everything generated `baml_sdk.*` code imports from the bridge lives here:
# PyO3 runtime classes (re-exported from `baml_core.baml_py`), the protobuf
# encoder/decoder, the three factory entry points, and `get_runtime()`.

import atexit
from typing import Any, Callable, Dict, List, Literal, Optional, Sequence

from .baml_py import (
    AbortController,
    BamlPyHandle,
    BamlRuntime,
    Collector as _RustCollector,
    FunctionLog as _RustFunctionLog,
    FunctionResult,
    HostSpanManager,
    LLMCall,
    Timing,
    Usage,
    flush_events,
    get_runtime as _rust_get_runtime,
    get_version,
)
from .errors import BamlError, BamlPanic, make_sdk_panic
from ._stream import BamlStream
from .ctx_manager import CtxManager as BamlCtxManager
from .proto import decode_call_result, encode_call_args
from .typemap import (
    BamlTypeMap,
    set_type_map,
    get_type_map,
)


# Flush buffered trace events on process exit so nothing is lost.
atexit.register(flush_events)

__version__ = "0.11.0"


# ---------------------------------------------------------------------------
# FunctionLog / Collector wrappers (decode protobuf result → Python value)
# ---------------------------------------------------------------------------


def _wrap_log(log: _RustFunctionLog) -> "FunctionLog":
    return FunctionLog(log)


class FunctionLog:
    """Python wrapper around the Rust FunctionLog that decodes the proto result."""

    __slots__ = ("_inner",)

    def __init__(self, inner: _RustFunctionLog):
        self._inner = inner

    @property
    def id(self) -> str:
        return self._inner.id

    @property
    def function_name(self) -> str:
        return self._inner.function_name

    @property
    def timing(self) -> Timing:
        return self._inner.timing

    @property
    def usage(self) -> Usage:
        return self._inner.usage

    @property
    def calls(self) -> List[LLMCall]:
        return self._inner.calls

    @property
    def tags(self) -> Dict[str, str]:
        return self._inner.tags

    @property
    def result(self) -> Optional[Any]:
        proto_bytes = self._inner.result
        if proto_bytes is None:
            return None
        return decode_call_result(proto_bytes)

    def __repr__(self):
        return repr(self._inner)


class Collector(_RustCollector):
    """Python subclass of the Rust Collector that wraps FunctionLog results.

    Overrides return the Python FunctionLog wrapper (which wraps the Rust
    FunctionLog), so pyright sees a nominal type mismatch — suppress it.
    """

    @property
    def logs(self) -> List["FunctionLog"]:  # type: ignore[override]
        return [_wrap_log(log) for log in super().logs]

    @property
    def last(self) -> Optional["FunctionLog"]:  # type: ignore[override]
        if last := super().last:
            return _wrap_log(last)
        return None

    def id(self, function_log_id: str) -> Optional["FunctionLog"]:  # type: ignore[override]
        if id := super().id(function_log_id):
            return _wrap_log(id)
        return None


# ---------------------------------------------------------------------------
# Runtime accessor
# ---------------------------------------------------------------------------


def get_runtime() -> BamlRuntime:
    """Return the process-global `BamlRuntime` singleton, or raise
    `BamlError` if `BamlRuntime.initialize_runtime(...)` has not run yet."""
    return _rust_get_runtime()


# ---------------------------------------------------------------------------
# call_function / call_function_sync — explicit-runtime helpers kept for
# the bridge tests. Generated code uses the three-arg factories below
# instead, which fetch the runtime lazily via `get_runtime()`.
# ---------------------------------------------------------------------------


def call_function_sync(rt, function_name, kwargs, ctx=None, collectors=None, abort_controller=None):
    args_proto = encode_call_args(kwargs)
    result_bytes = rt.call_function_sync(
        function_name, args_proto, ctx, collectors, abort_controller
    )
    return FunctionResult(decode_call_result(result_bytes))


async def call_function(rt, function_name, kwargs, ctx=None, collectors=None, abort_controller=None):
    args_proto = encode_call_args(kwargs)
    result_bytes = await rt.call_function(
        function_name, args_proto, ctx, collectors, abort_controller
    )
    return FunctionResult(decode_call_result(result_bytes))


# ---------------------------------------------------------------------------
# Factories consumed by generated `baml_sdk.*` leaves.
# Every factory captures `param_names` by closure; no runtime lookup on
# the call path (09b2 §2). The runtime is fetched lazily via
# `get_runtime()`, so constructing a factory has no sequencing constraint
# relative to `BamlRuntime.initialize_runtime(...)`.
# ---------------------------------------------------------------------------

Mode = Literal["sync", "async"]


class Unset:
    """Sentinel type for explicitly omitted generated SDK arguments."""
    __slots__ = ()
    _instance: "Unset | None" = None

    def __new__(cls) -> "Unset":
        if cls._instance is None:
            cls._instance = super().__new__(cls)
        return cls._instance

    def __repr__(self) -> str:
        return "baml.UNSET"


UNSET = Unset()


def _build_kwargs(
    args: Sequence[Any],
    kwargs: Dict[str, Any],
    required_param_names: List[str],
    optional_param_names: List[str],
) -> Dict[str, Any]:
    """Zip positional args with required names, then merge
    caller-supplied kwargs on top. Extra positional args error loudly
    so callers see a TypeError at the callsite, not a missing-argument
    error deep in the bridge."""
    positional_limit = len(required_param_names)
    if len(args) > positional_limit:
        raise TypeError(
            f"got {len(args)} positional arguments but only "
            f"{positional_limit} positional parameter names "
            f"({required_param_names!r})"
        )
    built: Dict[str, Any] = {}
    for name, value in zip(required_param_names, args):
        built[name] = value
    for k, v in kwargs.items():
        if v is UNSET:
            continue
        if k in built:
            raise TypeError(f"multiple values for argument {k!r}")
        built[k] = v
    return built


def define_function(
    baml_fqn: str,
    mode: Mode,
    required_param_names: List[str],
    optional_param_names: Optional[List[str]] = None,
) -> Callable[..., Any]:
    """Factory for a BAML callable (free function, static method, or
    instance method). Captures the call contract by closure; returns a
    callable that zips positional args against `required_param_names`,
    accepts optional parameters by keyword, encodes them, and hands the
    result to `decode_call_result`.

    For instance methods, `required_param_names[0]` is `"self"` — Python's
    descriptor protocol supplies the receiver as positional arg 0 when
    the returned callable is installed as a class attribute. Static
    methods are wrapped in `staticmethod(...)` by codegen to suppress
    that injection.
    """
    # Codegen always emits fully-qualified `<pkg>.<ns…>.<name>` FQNs and
    # the engine stores user functions under the same form (see
    # `12a-namespace-rules.md §5`); no translation step needed.
    required_names = list(required_param_names)
    optional_names = list(optional_param_names or [])
    if mode == "sync":
        def _sync(*args: Any, **kwargs: Any) -> Any:
            merged = _build_kwargs(args, kwargs, required_names, optional_names)
            rt = get_runtime()
            args_proto = encode_call_args(merged)
            result_bytes = rt.call_function_sync(baml_fqn, args_proto, None, None, None)
            return decode_call_result(result_bytes)
        return _sync
    elif mode == "async":
        async def _async(*args: Any, **kwargs: Any) -> Any:
            merged = _build_kwargs(args, kwargs, required_names, optional_names)
            rt = get_runtime()
            args_proto = encode_call_args(merged)
            result_bytes = await rt.call_function(baml_fqn, args_proto, None, None, None)
            return decode_call_result(result_bytes)
        return _async
    else:
        raise ValueError(f"mode must be 'sync' or 'async', got {mode!r}")


__all__ = [
    "AbortController",
    "BamlPyHandle",
    "BamlRuntime",
    "BamlStream",
    "Collector",
    "FunctionLog",
    "FunctionResult",
    "HostSpanManager",
    "LLMCall",
    "Timing",
    "Unset",
    "UNSET",
    "Usage",
    "BamlCtxManager",
    "BamlError",
    "BamlPanic",
    "make_sdk_panic",
    "flush_events",
    "get_runtime",
    "get_version",
    "encode_call_args",
    "decode_call_result",
    "call_function",
    "call_function_sync",
    "define_function",
    "BamlTypeMap",
    "set_type_map",
    "get_type_map",
]
