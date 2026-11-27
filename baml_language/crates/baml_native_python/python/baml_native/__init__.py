"""BAML compiled to machine code, called from Python.

What generated code imports in place of today's `baml_bridge` runtime:
`load` (in place of `BamlRuntime.initialize_runtime_from_blob`),
`set_type_map` and `define_function`, with the signatures today's have.

Results are ordinary instances of the generated classes, copied once from
BAML memory by the native module. Errors are today's SDK's own classes
(`baml_bridge.errors`), so code that catches them does not change.
"""

import os
import traceback
from typing import Any, Callable, Dict, List, Optional

from baml_bridge import UNSET, default_unhandled_spawn_error_handler
from baml_bridge import set_type_map as _bridge_set_type_map
from baml_bridge.errors import BamlError, BamlPanic, make_sdk_panic
from baml_bridge.typemap import BamlClassInstance

from . import _native

UnhandledSpawnErrorHandler = Callable[[BaseException, bool], None]

# Every call of a function the library does not export, by name, in order:
# how a test runner tells the tests of refused functions, whatever they
# catch.
_refused_calls: List[str] = []

_unhandled_spawn_error_handler: UnhandledSpawnErrorHandler = default_unhandled_spawn_error_handler


def set_unhandled_spawn_error_handler(
    handler: Optional[UnhandledSpawnErrorHandler],
) -> UnhandledSpawnErrorHandler:
    """As `baml_bridge.set_unhandled_spawn_error_handler`: what is done with
    the error of a spawned task nothing awaited. The default prints it and
    ends the process. Here the handler runs on the thread of the event loop
    (or of the sync call) that gave the library the turn in which the task
    failed, after that turn, with nothing of the library held."""
    global _unhandled_spawn_error_handler
    previous = _unhandled_spawn_error_handler
    _unhandled_spawn_error_handler = (
        default_unhandled_spawn_error_handler if handler is None else handler
    )
    return previous


def _report_unhandled(error: BaseException) -> None:
    try:
        _unhandled_spawn_error_handler(error, False)
    except BaseException:
        traceback.print_exception(error)
        raise


_native.configure(BamlError, BamlPanic, make_sdk_panic, BamlClassInstance, UNSET, _report_unhandled)


def load(path: "os.PathLike[str] | str", interface_hash: int) -> None:
    """Load the native library at `path`, refusing one generated for another
    version of the program. Once per process: the thread that loads it is
    the one that may call it."""
    _native.load(os.fspath(path), interface_hash)


def set_type_map(type_map: Any) -> None:
    """The generated type map, through which each class and enum of the
    library's interface finds its Python class. It is also today's
    runtime's type map, which the reflection surface reads."""
    _native.set_type_map(type_map)
    _bridge_set_type_map(type_map)


def define_function(
    baml_fqn: str,
    mode: str,
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
    """As `baml_bridge.define_function`. A function the library does not
    export (the packer refused it) still imports, and raises
    `NotImplementedError` when called."""
    if mode not in ("sync", "async"):
        raise ValueError(f"mode must be 'sync' or 'async', got {mode!r}")
    name = baml_fqn[len("user.") :] if baml_fqn.startswith("user.") else baml_fqn
    # Whether the function is in the library is the library's interface's
    # answer, nothing else.
    function: Any = _native.function(
        name, mode == "sync", list(required_param_names), dict(param_aliases or {})
    )
    if function is None:
        message = f"{baml_fqn} is not in the native library (the packer refused it)"
        if mode == "sync":

            def function(*args: Any, **kwargs: Any) -> Any:
                _refused_calls.append(baml_fqn)
                raise NotImplementedError(message)

        else:

            async def function(*args: Any, **kwargs: Any) -> Any:
                _refused_calls.append(baml_fqn)
                raise NotImplementedError(message)

    if binding_name is not None:
        function.__name__ = binding_name
    if binding_qualname is not None:
        function.__qualname__ = binding_qualname
    if binding_module is not None:
        function.__module__ = binding_module
    return function


# Measurement switches.
set_model_mode = _native.set_model_mode
set_release_gil = _native.set_release_gil
# Test aids.
handle_count = _native.handle_count
collect = _native.collect
calls_in_flight = _native.calls_in_flight
panic_next = _native.panic_next
build_model = _native.build_model

__all__ = [
    "load",
    "set_type_map",
    "define_function",
    "set_unhandled_spawn_error_handler",
    "UnhandledSpawnErrorHandler",
]
