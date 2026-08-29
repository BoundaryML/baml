# BAML Python runtime surface — the `baml_bridge` package.
#
# Everything generated `baml_sdk.*` code imports from the bridge lives here:
# PyO3 runtime classes (re-exported from `baml_bridge.baml_py`), the protobuf
# encoder/decoder, the three factory entry points, and `get_runtime()`.

import atexit
import asyncio
import functools
import os
import signal
import sys
import traceback
from typing import Any, Callable, Dict, List, Literal, Optional, Sequence

from typing_extensions import Sentinel

from .baml_py import (
    BamlCallContext,
    BamlPyHandle,
    BamlRuntime,
    Collector as _RustCollector,
    FunctionLog as _RustFunctionLog,
    FunctionResult,
    HostSpanManager,
    LLMCall,
    Timing,
    Usage,
    cancel_function_call,
    flush_events,
    get_runtime as _rust_get_runtime,
    get_bridge_runtime_version,
    get_toolchain_version,
    get_version,
    new_function_call,
    register_unhandled_spawn_error_callback,
    shutdown_runtime,
)
from .errors import (
    BamlCancelledError,
    BamlError,
    BamlPanic,
    attach_baml_traceback,
    make_sdk_panic,
)
from ._stream import BamlStream
from ._function_spec import BamlFunctionSpec
from ._runtime_value import BamlRuntimeValue
from .ctx_manager import CtxManager as BamlCtxManager
from .proto import (
    BamlType,
    decode_call_result,
    encode_call_args,
    pydantic_instance_type_args,
    python_type_to_wire_ty,
)
from .typemap import (
    BamlTypeMap,
    set_type_map,
    get_type_map,
)


# Complete spawned work before flushing buffered trace events.
atexit.register(flush_events)
atexit.register(shutdown_runtime)


def _handle_unhandled_spawn_error(error_bytes: bytes, cancelled: bool) -> None:
    try:
        decode_call_result(error_bytes)
    except BaseException as error:
        traceback.print_exception(error)
        if not cancelled:
            sys.stderr.flush()
            if os.name == "nt":
                os.kill(os.getpid(), signal.SIGTERM)
            else:
                os._exit(1)


register_unhandled_spawn_error_callback(_handle_unhandled_spawn_error)

__version__ = "0.17.0"


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


_CANCELLED_PANIC_CLASS = "baml.panics.Cancelled"


def _attach_call_ctx(call_ctx: Any, call_id: int) -> None:
    if call_ctx is not None:
        call_ctx._attach_call_id(call_id)


def _detach_call_ctx(call_ctx: Any, call_id: int) -> None:
    if call_ctx is not None:
        call_ctx._detach_call_id(call_id)


def _decode_call_result_async(result_bytes: bytes) -> Any:
    try:
        return decode_call_result(result_bytes)
    except (BamlError, BamlPanic) as exc:
        if getattr(exc, "class_name", None) != _CANCELLED_PANIC_CLASS:
            raise
        reason = BamlCancelledError(
            exc.value,
            baml_trace=exc.baml_trace,
            class_name=getattr(exc, "class_name", None),
        )
        cancelled = asyncio.CancelledError(str(reason))
        cancelled.reason = reason  # type: ignore[attr-defined]
        raise attach_baml_traceback(cancelled) from exc


# ---------------------------------------------------------------------------
# call_function / call_function_sync — explicit-runtime helpers kept for
# the bridge tests. Generated code uses the three-arg factories below
# instead, which fetch the runtime lazily via `get_runtime()`.
# ---------------------------------------------------------------------------


def call_function_sync(rt, function_name, kwargs, ctx=None, collectors=None, _ctx=None):
    call_id = new_function_call()
    args_proto = encode_call_args(kwargs, call_id, function_name=function_name)
    _attach_call_ctx(_ctx, call_id)
    try:
        result_bytes = rt.call_function_sync(args_proto, ctx, collectors)
    finally:
        _detach_call_ctx(_ctx, call_id)
    return FunctionResult(decode_call_result(result_bytes))


async def call_function(
    rt, function_name, kwargs, ctx=None, collectors=None, _ctx=None
):
    call_id = new_function_call()
    args_proto = encode_call_args(kwargs, call_id, function_name=function_name)
    _attach_call_ctx(_ctx, call_id)
    try:
        try:
            result_bytes = await rt.call_function(args_proto, ctx, collectors)
        except asyncio.CancelledError:
            cancel_function_call(call_id)
            raise
    finally:
        _detach_call_ctx(_ctx, call_id)
    return FunctionResult(_decode_call_result_async(result_bytes))


# ---------------------------------------------------------------------------
# Factories consumed by generated `baml_sdk.*` leaves.
# Every factory captures `param_names` by closure; no runtime lookup on
# the call path (09b2 §2). The runtime is fetched lazily via
# `get_runtime()`, so constructing a factory has no sequencing constraint
# relative to `BamlRuntime.initialize_runtime(...)`.
# ---------------------------------------------------------------------------

Mode = Literal["sync", "async"]


# Sentinel for explicitly omitted generated SDK arguments. `Sentinel`
# (PEP 661, via `typing_extensions`) yields a single value that doubles as
# its own type, so generated `.pyi` stubs can write
# `opt: typing.Union[int, None, UNSET] = UNSET` — both the annotation and
# the default are this one object. Type checkers only recognize a sentinel
# in a type expression when it is referenced by a *bare name*, so generated
# code imports `UNSET` directly rather than reaching it via attribute access.
UNSET = Sentinel("UNSET")


def _build_kwargs(
    args: Sequence[Any],
    kwargs: Dict[str, Any],
    required_param_names: List[str],
    optional_param_names: List[str],
    param_aliases: Optional[Dict[str, str]] = None,
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
    aliases = param_aliases or {}
    built: Dict[str, Any] = {}
    for name, value in zip(required_param_names, args):
        built[aliases.get(name, name)] = value
    for k, v in kwargs.items():
        if v is UNSET:
            continue
        wire_name = aliases.get(k, k)
        if wire_name in built:
            raise TypeError(f"multiple values for argument {k!r}")
        built[wire_name] = v
    return built


def _resolve_types_kwarg(types_kwarg: Any, type_params: List[str]) -> List[Any]:
    """Map the user-facing `_types=` value onto the callee's own generic params,
    in declaration order. Each slot is the bound type, or `None` when the caller
    left it for the engine to **infer** from the argument values.

    `_types=` is a `{param_name: type}` dict and is now **optional**: the engine
    solves any TypeVar a value can carry (inbound-inference, 01a/01b), so a bare
    generic call (no `_types=`, no subscript) is legal. `_types=` is still the
    *only* way to bind a TypeVar no value carries (return/body-only params) and
    the only accepted *shape* (the legacy single-type / positional tuple/list
    forms are gone). A partial dict is allowed: named params bind explicitly, the
    rest infer. Class type params (bound from a generic receiver) are *not* part
    of `_types=`. An omitted/partial binding that leaves a genuinely
    uninferable TypeVar unbound is rejected by the **engine** (Gate A), not here.
    """
    if not type_params:
        # No own generic params: `_types=` must not be supplied. (Class type
        # params, if any, ride the receiver instance, not `_types=`.)
        if types_kwarg is not None:
            raise TypeError(
                "_types= is not accepted here: this function/method declares no "
                "generic type parameters of its own"
            )
        return []
    example = f"{{{type_params[0]!r}: int}}"
    if types_kwarg is None:
        # No explicit bindings — infer every param from the argument values.
        return [None for _ in type_params]
    if not isinstance(types_kwarg, dict):
        raise TypeError(
            f"_types= must be a dict mapping type-parameter names to types "
            f"(e.g. _types={example}); got {type(types_kwarg).__name__}. The "
            f"single-type and positional tuple/list forms are no longer accepted."
        )
    extra = [k for k in types_kwarg if k not in type_params]
    if extra:
        raise TypeError(
            f"_types= has unknown type parameter(s) {extra!r}; expected exactly "
            f"{type_params!r}."
        )
    # Missing params are inferred by the engine, not an error: a partial dict
    # binds what it names and leaves the rest (`None`) to inference.
    return [types_kwarg.get(name) for name in type_params]


def _build_type_args(
    merged: Dict[str, Any],
    types_kwarg: Any,
    type_params: List[str],
    class_type_params: List[str],
) -> List[Any]:
    """Build the named, order-preserving `BamlTyArg` list for a generic call: each
    entry is `(type_var_name, wire_ty)`. Enclosing class params (recovered from
    the `self` receiver's Pydantic generic metadata) come first, then the
    callee's own `<...>` params (`_types=`) — De Bruijn order. The engine maps
    each entry onto the entry frame's `type_args` slot by TypeVar name. Returns
    `[]` when the call binds nothing.

    Class params are seeded only when concrete args are actually recovered from
    a Pydantic generic *instance*. For builtin/handle receivers (no Pydantic
    generic metadata) nothing is sent, so the engine keeps recovering class
    type args from the receiver itself — preserving pre-existing behavior for
    stdlib generic methods (`baml.llm.Array`, `Stream`, …)."""
    wire: List[Any] = []
    class_args = (
        pydantic_instance_type_args(merged.get("self")) if class_type_params else []
    )
    if class_args:
        for i, name in enumerate(class_type_params):
            arg = class_args[i] if i < len(class_args) else None
            wire.append(
                (
                    name,
                    arg if isinstance(arg, BamlType) else python_type_to_wire_ty(arg),
                )
            )

    resolved = _resolve_types_kwarg(types_kwarg, type_params)
    # Send only the *explicitly* bound params (non-`None`); the rest are inferred
    # engine-side from the argument values. A partially-bound generic call sends
    # a partial `BamlTyArg` list and the engine fills the gaps.
    bound = [(name, r) for name, r in zip(type_params, resolved) if r is not None]
    if bound:
        if class_type_params and not class_args:
            # The method's own params sit *after* the class prefix in De Bruijn
            # order; without recovered class args we can't position an
            # explicitly-bound one. This combined shape (a method-level TypeVar
            # bound via `_types=` on a non-Pydantic generic receiver) isn't
            # supported yet. (A *bare* such call infers fine — `bound` is empty.)
            raise TypeError(
                "_types= on a generic method requires a Pydantic generic "
                "receiver so the class type args can be recovered"
            )
        wire.extend(
            (name, r if isinstance(r, BamlType) else python_type_to_wire_ty(r))
            for name, r in bound
        )
    return wire


class _GenericCallable(staticmethod):
    """Subscriptable wrapper for a generic free function / method (Phase 6).

    `fn[X, Y](...)` is pure sugar for `fn(..., _types={p0: X, p1: Y})`, where
    `p0, p1, …` are the callee's OWN generic params (`type_param_names`, in
    declaration order). The subscript binds them positionally; the resulting
    partial delegates to the same `_types=`-based call path as Phase 4/5, so the
    two forms produce identical wire payloads. The explicit `_types=` form keeps
    working via `__call__` (subscript is sugar, not a replacement).

    It subclasses `staticmethod` for two reasons: (1) Pydantic ignores
    `staticmethod` attributes, so a generic *instance* method bound bare in a
    generated `pydantic.BaseModel` class body isn't mistaken for a model field;
    (2) it carries the wrapped callable in `__func__`. The overridden `__get__`
    *does* bind the receiver (unlike a plain `staticmethod`) so a generic
    instance method works as `k.method(...)` / `k.method[U](...)`. A true static
    method is wrapped by codegen in an outer `staticmethod(...)`, whose `__get__`
    returns this object unbound — so no receiver is injected there.
    """

    def __new__(cls, call: Callable[..., Any], type_param_names: List[str]):
        return super().__new__(cls, call)

    def __init__(self, call: Callable[..., Any], type_param_names: List[str]) -> None:
        super().__init__(call)
        self._type_param_names = type_param_names
        for attr in functools.WRAPPER_ASSIGNMENTS:
            if hasattr(call, attr):
                setattr(self, attr, getattr(call, attr))

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        return self.__func__(*args, **kwargs)

    def __getitem__(self, key: Any) -> Callable[..., Any]:
        types = key if isinstance(key, tuple) else (key,)
        if len(types) != len(self._type_param_names):
            raise TypeError(
                f"expected {len(self._type_param_names)} type argument(s) for "
                f"{self._type_param_names!r}, got {len(types)}"
            )
        bound = dict(zip(self._type_param_names, types))
        return functools.partial(self.__func__, _types=bound)

    def __get__(self, obj: Any, objtype: Any = None) -> "_GenericCallable":
        # Unbound access (`Cls.method`) returns the wrapper itself; bound access
        # (`inst.method`) pre-binds the receiver so the subscript/`__call__`
        # forms both forward `self` like an ordinary bound method.
        if obj is None:
            return self
        bound = _GenericCallable(
            functools.partial(self.__func__, obj), self._type_param_names
        )
        for attr in ("__name__", "__qualname__", "__module__", "__doc__"):
            setattr(bound, attr, getattr(self, attr))
        return bound


def _maybe_generic_callable(
    call: Callable[..., Any], own_type_params: List[str]
) -> Callable[..., Any]:
    """Wrap `call` so it accepts the `fn[...]` subscript form when the callee
    declares its OWN generic params; otherwise return it unchanged (non-generic
    functions and methods whose only TypeVars ride the receiver stay plain
    callables, avoiding any behavior change)."""
    if own_type_params:
        return _GenericCallable(call, own_type_params)
    return call


def define_function(
    baml_fqn: str,
    mode: Mode,
    required_param_names: List[str],
    optional_param_names: Optional[List[str]] = None,
    *,
    type_params: Optional[List[str]] = None,
    class_type_params: Optional[List[str]] = None,
    param_aliases: Optional[Dict[str, str]] = None,
    binding_name: Optional[str] = None,
    binding_qualname: Optional[str] = None,
    binding_module: Optional[str] = None,
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

    Generics: `type_params` are the callee's own `<...>` param names (bound
    by an explicit `_types=` kwarg), and `class_type_params` are the
    enclosing generic class's param names (bound from the `self` receiver's
    runtime type args). When either is set, a named, order-preserving `BamlTyArg`
    list (`(type_var, type_value)` per TypeVar) is sent in
    `CallFunctionArgs.type_args` for the engine to seed the entry frame.
    """
    # Codegen always emits fully-qualified `<pkg>.<ns…>.<name>` FQNs and
    # the engine stores user functions under the same form (see
    # `12a-namespace-rules.md §5`); no translation step needed.
    required_names = list(required_param_names)
    optional_names = list(optional_param_names or [])
    type_param_names = list(type_params or [])
    class_type_param_names = list(class_type_params or [])
    host_to_wire_param_names = dict(param_aliases or {})
    is_generic = bool(type_param_names or class_type_param_names)

    def _set_binding_metadata(call: Callable[..., Any]) -> None:
        if binding_name is not None:
            call.__name__ = binding_name
        if binding_qualname is not None:
            call.__qualname__ = binding_qualname
        if binding_module is not None:
            call.__module__ = binding_module

    if mode == "sync":

        def _sync(*args: Any, **kwargs: Any) -> Any:
            call_ctx = kwargs.pop("_ctx", None)
            types_kwarg = kwargs.pop("_types", None)
            merged = _build_kwargs(
                args,
                kwargs,
                required_names,
                optional_names,
                host_to_wire_param_names,
            )
            call_kwargs = merged
            type_args = (
                _build_type_args(
                    call_kwargs,
                    types_kwarg,
                    type_param_names,
                    class_type_param_names,
                )
                if is_generic
                else None
            )
            rt = get_runtime()
            call_id = new_function_call()
            args_proto = encode_call_args(
                call_kwargs,
                call_id,
                type_args,
                function_name=baml_fqn,
            )
            _attach_call_ctx(call_ctx, call_id)
            try:
                result_bytes = rt.call_function_sync(args_proto, None, None)
            finally:
                _detach_call_ctx(call_ctx, call_id)
            return decode_call_result(result_bytes)

        _set_binding_metadata(_sync)
        return _maybe_generic_callable(_sync, type_param_names)
    elif mode == "async":

        async def _async(*args: Any, **kwargs: Any) -> Any:
            call_ctx = kwargs.pop("_ctx", None)
            types_kwarg = kwargs.pop("_types", None)
            merged = _build_kwargs(
                args,
                kwargs,
                required_names,
                optional_names,
                host_to_wire_param_names,
            )
            call_kwargs = merged
            type_args = (
                _build_type_args(
                    call_kwargs,
                    types_kwarg,
                    type_param_names,
                    class_type_param_names,
                )
                if is_generic
                else None
            )
            rt = get_runtime()
            call_id = new_function_call()
            args_proto = encode_call_args(
                call_kwargs,
                call_id,
                type_args,
                function_name=baml_fqn,
            )
            _attach_call_ctx(call_ctx, call_id)
            try:
                try:
                    result_bytes = await rt.call_function(args_proto, None, None)
                except asyncio.CancelledError:
                    cancel_function_call(call_id)
                    raise
            finally:
                _detach_call_ctx(call_ctx, call_id)
            return _decode_call_result_async(result_bytes)

        _set_binding_metadata(_async)
        return _maybe_generic_callable(_async, type_param_names)
    else:
        raise ValueError(f"mode must be 'sync' or 'async', got {mode!r}")


__all__ = [
    "BamlCallContext",
    "BamlPyHandle",
    "BamlRuntime",
    "BamlType",
    "BamlStream",
    "BamlFunctionSpec",
    "BamlRuntimeValue",
    "Collector",
    "FunctionLog",
    "FunctionResult",
    "HostSpanManager",
    "LLMCall",
    "Timing",
    "UNSET",
    "Usage",
    "BamlCtxManager",
    "BamlCancelledError",
    "BamlError",
    "BamlPanic",
    "make_sdk_panic",
    "flush_events",
    "shutdown_runtime",
    "get_runtime",
    "get_version",
    "new_function_call",
    "cancel_function_call",
    "encode_call_args",
    "decode_call_result",
    "call_function",
    "call_function_sync",
    "define_function",
    "BamlTypeMap",
    "set_type_map",
    "get_type_map",
]
