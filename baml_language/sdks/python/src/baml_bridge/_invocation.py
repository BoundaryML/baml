"""Invocation normalization and experimental dynamic calls; generated SDKs supply types."""

from __future__ import annotations

from typing import Any

from .baml_py import (
    BamlPyHandle,
    _trace_selection,
    invocation_clock_ns,
)
from ._execution_context import _current_trace_options, current
from .cffi.v1 import baml_inbound_pb2


def normalize(options: Any, call_id: int):
    """Snapshot controls before encoding application values; retain capabilities."""
    from .typemap import get_type_map

    if options is None:
        snapshot = {}
    elif isinstance(options, dict):
        snapshot = dict(options)
    else:
        raise TypeError("_baml must be a dictionary or None")
    unknown = snapshot.keys() - {"trace", "cancel", "timeout_ms"}
    if unknown:
        raise TypeError(f"unknown _baml controls: {sorted(unknown, key=str)!r}")
    timeout = snapshot.get("timeout_ms")
    if timeout is not None and (
        isinstance(timeout, bool)
        or not isinstance(timeout, int)
        or not 0 <= timeout <= 2_147_483_647
    ):
        raise ValueError("timeout_ms must be an integer in 0…2147483647 or None")
    wire = baml_inbound_pb2.InvocationOptions(host_environment=call_id)
    if timeout is not None:
        # Freeze the deadline before any value conversion, trace inspection, or
        # encoding work. The allocated runtime supplies the monotonic clock.
        deadline = invocation_clock_ns(call_id) + timeout * 1_000_000
        if deadline > (1 << 64) - 1:
            raise ValueError("timeout deadline exceeds the runtime clock range")
        wire.deadline_ns = deadline
    active = current()
    if active is not None:
        wire.inherited_state = active._key_for_invocation()
    retained = [active]
    if "trace" not in snapshot:
        for handle in _current_trace_options.get():
            selection, reservation = _trace_selection(handle, call_id)
            inherited = baml_inbound_pb2.TraceSelection.FromString(bytes(selection))
            # Only mode/capture are defaults. Tags already flow through the
            # runtime state; replaying their patches would overwrite contexts
            # established inside BAML before a Python callback.
            inherited.options.ClearField("distinct_id")
            inherited.options.ClearField("metadata")
            wire.trace.MergeFrom(inherited)
            retained.extend((handle, reservation))
    trace = snapshot.get("trace")
    if trace is not None:
        type_map = get_type_map()
        if not isinstance(
            trace,
            (
                type_map.get_class("trace.Options"),
                type_map.get_class("trace.ReservedSpan"),
            ),
        ):
            raise TypeError(
                "trace must be generated trace.Options or trace.ReservedSpan"
            )
        handle = next(
            (
                getattr(trace, name)
                for name, field in type(trace).model_fields.items()
                if (field.serialization_alias or field.alias or name) == "_handle"
            ),
            None,
        )
        if not isinstance(handle, BamlPyHandle):
            raise TypeError("trace value must contain a live generated handle")
        selection, reservation = _trace_selection(handle, call_id)
        wire.trace.ParseFromString(bytes(selection))
        retained.extend((trace, reservation))
    cancel = snapshot.get("cancel")
    if cancel is not None:
        if not isinstance(cancel, get_type_map().get_class("baml.spawn.CancelToken")):
            raise TypeError("cancel must be a generated baml.spawn.CancelToken")
        retained.append(cancel)
    # Token conversion is performed by encode_call_args inside its ownership
    # rollback guard, alongside ordinary application values.
    return wire, cancel, retained


def _dynamic_args(target, arguments, types, options):
    from .baml_py import new_function_call
    from .proto import BamlClosure, BamlType, encode_call_args, python_type_to_wire_ty

    if not isinstance(arguments, dict) or any(
        not isinstance(key, str) for key in arguments
    ):
        raise TypeError("arguments must be a dictionary with string keys")
    if types is not None and not isinstance(types, dict):
        raise TypeError("_types must be a dictionary or None")
    if isinstance(target, BamlClosure):
        if types is not None:
            raise TypeError("specialized BAML callable handles do not accept _types")
        call_target = {"function_handle": target._to_pyhandle()._key_for_call()}
    elif isinstance(target, str):
        call_target = {"function_name": target}
    else:
        raise TypeError(
            "target must be a fully qualified BAML name or returned callable"
        )
    bindings = [
        (name, value if isinstance(value, BamlType) else python_type_to_wire_ty(value))
        for name, value in (types or {}).items()
        if value is not None
    ]
    call_id = new_function_call()
    return call_id, encode_call_args(
        arguments, call_id, bindings, _baml=options, **call_target
    )


def invoke(target, arguments, *, _types=None, _baml=None):
    from . import get_runtime
    from .proto import decode_call_result

    _, encoded = _dynamic_args(target, arguments, _types, _baml)
    if isinstance(target, str) and target.endswith("@stream"):
        from ._stream import _decode_stream_result

        return _decode_stream_result(
            get_runtime().call_function_sync(encoded, stream=True)
        )
    return decode_call_result(get_runtime().call_function_sync(encoded))


async def invoke_async(target, arguments, *, _types=None, _baml=None):
    import asyncio
    from . import _decode_call_result_async, get_runtime
    from .baml_py import cancel_function_call

    call_id, encoded = _dynamic_args(target, arguments, _types, _baml)
    try:
        if isinstance(target, str) and target.endswith("@stream"):
            result = await get_runtime().call_function(encoded, stream=True)
        else:
            result = await get_runtime().call_function(encoded)
    except asyncio.CancelledError:
        cancel_function_call(call_id)
        raise
    if isinstance(target, str) and target.endswith("@stream"):
        from ._stream import _decode_stream_result

        return _decode_stream_result(result, asynchronous=True)
    return _decode_call_result_async(result)
