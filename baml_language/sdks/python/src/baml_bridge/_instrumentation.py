"""Host bodies run inline; the runtime owns context and recording lifecycle."""

from __future__ import annotations

import asyncio
import functools
import inspect
import logging
import sys
import threading

from ._dispatch import _current_invocation
from ._invocation import _Invocation
from .baml_py import _begin_host_invocation, _validate_host_options, BamlPyHandle
from .typemap import get_type_map


class TraceUsageError(TypeError):
    """Invalid marker configuration or a recognizably unsupported host shape."""


def _options_handle(options):
    if options is None:
        return None
    if not isinstance(options, get_type_map().get_class("trace.Options")):
        raise TraceUsageError("instrument accepts generated trace.Options or None")
    handle = next(
        (
            getattr(options, name)
            for name, field in type(options).model_fields.items()
            if (field.serialization_alias or field.alias or name) == "_handle"
        ),
        None,
    )
    if not isinstance(handle, BamlPyHandle):
        raise TraceUsageError("instrument options must contain a live generated handle")
    return handle


def _enter(definition, options, caller, inputs):
    active = _current_invocation.get()
    try:
        execution, state, cancel = _begin_host_invocation(
            definition,
            active._state if active is not None else None,
            options,
            caller,
            inputs,
        )
        return execution, _current_invocation.set(_Invocation((state, cancel)))
    except TypeError as error:
        raise TraceUsageError("invalid host execution context") from error
    except Exception:
        # One diagnostic per failure. Neither logging nor recording may replace
        # an application's return value or escaping exception.
        _diagnostic("host trace entry failed")
        return None, None


def _caller_site():
    try:
        frame = sys._getframe(2)
        return frame.f_code.co_filename, frame.f_lineno
    except ValueError:
        return "<native>", 0


_diagnostic_count = 0
_diagnostic_lock = threading.Lock()


def _diagnostic(message):
    global _diagnostic_count
    with _diagnostic_lock:
        if _diagnostic_count >= 8:
            return
        _diagnostic_count += 1
    try:
        logging.getLogger("baml.trace").warning(message)
    except Exception:
        pass


def _exit(execution, token, outcome, value):
    try:
        if execution is not None:
            try:
                execution.finish(outcome, value)
            except Exception:
                _diagnostic("host trace completion failed")
    finally:
        if token is not None:
            _current_invocation.reset(token)


def _exception_capture(error):
    try:
        return {
            "type": type.__getattribute__(type(error), "__qualname__"),
            "args": BaseException.args.__get__(error, type(error)),
        }
    except Exception:
        return None


def instrument(function_or_options=None, *, name=None):
    """Instrument an ordinary Python function or native coroutine function.

    Native generators and arbitrary callable objects are currently unsupported.
    Construction validates configuration but captures no execution context.
    """
    if name is not None and (not isinstance(name, str) or not name):
        raise TraceUsageError("instrument name must be a nonempty string or None")
    wrapper_line = _caller_site()[1]
    direct = inspect.isfunction(function_or_options)
    options = None if direct else _options_handle(function_or_options)
    try:
        capture_inputs, _, _ = _validate_host_options(options)
    except Exception as error:
        raise TraceUsageError("invalid host trace options") from error

    def decorate(function):
        if not inspect.isfunction(function):
            raise TraceUsageError(
                "instrument supports ordinary functions and async functions"
            )
        if inspect.isgeneratorfunction(function) or inspect.isasyncgenfunction(
            function
        ):
            raise TraceUsageError("generator instrumentation is not supported")
        original = inspect.unwrap(function)
        definition = (
            original.__module__,
            original.__qualname__,
            original.__code__.co_filename,
            original.__code__.co_firstlineno,
            wrapper_line,
            name if name is not None else original.__qualname__,
        )
        signature = inspect.signature(function)
        parameters = list(signature.parameters)
        receiver = (
            parameters
            and parameters[0] in ("self", "cls")
            and "." in original.__qualname__
            and not original.__qualname__.rsplit(".", 1)[0].endswith("<locals>")
        )

        def inputs(args, kwargs):
            if not capture_inputs:
                return None
            try:
                bound = signature.bind(*args, **kwargs)
                bound.apply_defaults()
                values = dict(bound.arguments)
                if receiver:
                    values.pop(parameters[0], None)
                return values
            except Exception:
                # The ordinary call still supplies its own argument error.
                return None

        if inspect.iscoroutinefunction(function):

            @functools.wraps(function)
            async def asynchronous(*args, **kwargs):
                execution, token = _enter(
                    definition, options, _caller_site(), inputs(args, kwargs)
                )
                outcome = "ok"
                result = None
                try:
                    result = await function(*args, **kwargs)
                    return result
                except asyncio.CancelledError:
                    outcome = "cancelled"
                    raise
                except BaseException as error:
                    outcome = "error"
                    result = _exception_capture(error)
                    raise
                finally:
                    _exit(execution, token, outcome, result)

            return asynchronous

        @functools.wraps(function)
        def synchronous(*args, **kwargs):
            execution, token = _enter(
                definition, options, _caller_site(), inputs(args, kwargs)
            )
            outcome = "ok"
            result = None
            try:
                result = function(*args, **kwargs)
                return result
            except asyncio.CancelledError:
                outcome = "cancelled"
                raise
            except BaseException as error:
                outcome = "error"
                result = _exception_capture(error)
                raise
            finally:
                _exit(execution, token, outcome, result)

        return synchronous

    return decorate(function_or_options) if direct else decorate
