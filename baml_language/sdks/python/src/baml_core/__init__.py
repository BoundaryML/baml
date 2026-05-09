# BAML Python runtime surface — the `baml_core` package.
#
# Everything generated `baml_sdk.*` code imports from the bridge lives here:
# PyO3 runtime classes (re-exported from `baml_core.baml_py`), the protobuf
# encoder/decoder, the three factory entry points, and `get_runtime()`.

import atexit
import importlib
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
from .baml_py import (
    BamlCancelledError,
    BamlClientError,
    BamlError,
    BamlInvalidArgumentError,
)
from ._stream import BamlStream
from .ctx_manager import CtxManager as BamlCtxManager
from .proto import decode_call_result, encode_call_args


# Flush buffered trace events on process exit so nothing is lost.
atexit.register(flush_events)


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
# Runtime accessor + sdk_root routing (consumed by the outbound decoder)
# ---------------------------------------------------------------------------


def get_runtime() -> BamlRuntime:
    """Return the process-global `BamlRuntime` singleton, or raise
    `BamlError` if `BamlRuntime.initialize_runtime(...)` has not run yet."""
    return _rust_get_runtime()


def _routed_python_path(baml_fqn: str) -> str:
    """Map a BAML FQN (e.g. ``user.lorem.Resume``) to a Python subpath
    under ``baml_sdk`` per 09b §1. Pure function; memoized below.

    Emit always fully qualifies, so the FQN's first segment is the
    literal package name: `user.*` → user-package leaves, `baml.*` →
    stdlib, anything else → `vendor/<pkg>/...`. See
    `12a-namespace-rules.md §5`.
    """
    pkg, _, tail = baml_fqn.partition(".")
    if not tail:
        raise ValueError(f"Unrecognised BAML FQN: {baml_fqn!r}")
    if pkg == "user":
        return tail
    if pkg == "baml":
        return f"baml.{tail}"
    return f"vendor.{baml_fqn}"


_RESOLVE_TYPE_CACHE: Dict[str, type] = {}


def _resolve_type(baml_type_expr: str) -> type:
    """Resolve a BAML type expression (FQN, optionally `$stream`-suffixed)
    to the Python class under `sdk_root`. Phase-4 extends this; phase 3
    only wires up the memoization + import path so the decoder has a
    seam to drop into.
    """
    cached = _RESOLVE_TYPE_CACHE.get(baml_type_expr)
    if cached is not None:
        return cached

    expr = baml_type_expr
    is_stream = False
    if expr.endswith("$stream"):
        expr = expr[: -len("$stream")]
        is_stream = True

    subpath = _routed_python_path(expr)
    if is_stream:
        subpath = f"stream_types.{subpath}"

    sdk_root = get_runtime()._sdk_root
    full = f"{sdk_root}.{subpath}"
    module_path, _, type_name = full.rpartition(".")
    module = importlib.import_module(module_path)
    resolved = getattr(module, type_name)
    _RESOLVE_TYPE_CACHE[baml_type_expr] = resolved
    return resolved


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
UNSET = object()


def _build_kwargs(
    args: Sequence[Any],
    kwargs: Dict[str, Any],
    param_names: List[str],
    required_positional_count: Optional[int] = None,
) -> Dict[str, Any]:
    """Zip positional args with captured `param_names`, then merge
    caller-supplied kwargs on top. Extra positional args error loudly
    so callers see a TypeError at the callsite, not a missing-argument
    error deep in the bridge."""
    positional_limit = (
        len(param_names)
        if required_positional_count is None
        else required_positional_count
    )
    if len(args) > positional_limit:
        raise TypeError(
            f"got {len(args)} positional arguments but only "
            f"{positional_limit} positional parameter names "
            f"({param_names[:positional_limit]!r})"
        )
    built: Dict[str, Any] = {}
    for name, value in zip(param_names, args):
        built[name] = value
    for k, v in kwargs.items():
        if v is UNSET:
            continue
        if k in built:
            raise TypeError(f"multiple values for argument {k!r}")
        built[k] = v
    return built


def _make_call(
    baml_fqn: str,
    mode: Mode,
    param_names: List[str],
    required_positional_count: Optional[int] = None,
) -> Callable[..., Any]:
    # Codegen always emits fully-qualified `<pkg>.<ns…>.<name>` FQNs and
    # the engine stores user functions under the same form (see
    # `12a-namespace-rules.md §5`); no translation step needed.
    names = list(param_names)
    if mode == "sync":
        def _sync(*args: Any, **kwargs: Any) -> Any:
            merged = _build_kwargs(args, kwargs, names, required_positional_count)
            rt = get_runtime()
            args_proto = encode_call_args(merged)
            result_bytes = rt.call_function_sync(baml_fqn, args_proto, None, None, None)
            return decode_call_result(result_bytes)

        _sync.__name__ = baml_fqn.rsplit(".", 1)[-1] or baml_fqn
        _sync.__qualname__ = _sync.__name__
        return _sync
    elif mode == "async":
        async def _async(*args: Any, **kwargs: Any) -> Any:
            merged = _build_kwargs(args, kwargs, names, required_positional_count)
            rt = get_runtime()
            args_proto = encode_call_args(merged)
            result_bytes = await rt.call_function(baml_fqn, args_proto, None, None, None)
            return decode_call_result(result_bytes)

        _async.__name__ = baml_fqn.rsplit(".", 1)[-1] or baml_fqn
        _async.__qualname__ = _async.__name__
        return _async
    else:
        raise ValueError(f"mode must be 'sync' or 'async', got {mode!r}")


def define_function(
    baml_fqn: str,
    mode: Mode,
    param_names: List[str],
    required_positional_count: Optional[int] = None,
) -> Callable[..., Any]:
    """Factory for a free BAML function. Captures the call contract by
    closure; returns a callable that zips positional args against
    `param_names`, encodes them, and hands the result to `decode_call_result`."""
    return _make_call(baml_fqn, mode, param_names, required_positional_count)


def define_static_method(
    baml_fqn: str,
    mode: Mode,
    param_names: List[str],
    required_positional_count: Optional[int] = None,
) -> Callable[..., Any]:
    """Factory for a BAML static method. Same contract as `define_function`
    today; a distinct name lets codegen route stdlib-shaped call sites
    without branching on string shape."""
    return _make_call(baml_fqn, mode, param_names, required_positional_count)


def define_instance_method(
    baml_fqn: str,
    mode: Mode,
    param_names: List[str],
    required_positional_count: Optional[int] = None,
) -> Callable[..., Any]:
    """Factory for a BAML instance method. `param_names[0]` is expected
    to be ``"self"``; Python's descriptor protocol supplies the receiver
    as positional arg 0 when the returned callable is installed as a
    class attribute."""
    return _make_call(baml_fqn, mode, param_names, required_positional_count)


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
    "UNSET",
    "Usage",
    "BamlCtxManager",
    "BamlError",
    "BamlCancelledError",
    "BamlClientError",
    "BamlInvalidArgumentError",
    "flush_events",
    "get_runtime",
    "get_version",
    "encode_call_args",
    "decode_call_result",
    "call_function",
    "call_function_sync",
    "define_function",
    "define_static_method",
    "define_instance_method",
]
