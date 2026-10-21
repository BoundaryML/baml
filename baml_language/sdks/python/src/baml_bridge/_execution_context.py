"""Private inheritable execution context; independent of execution lifetime."""

from contextlib import contextmanager
from contextvars import ContextVar

from .baml_py import _invocation_context
from .cffi.v1 import baml_outbound_pb2

_current_execution_context = ContextVar("baml_execution_context", default=None)
# Python instrumentation supplies defaults for calls made by its body. Keep
# this separate from runtime context: BAML context patches must still win on
# callback reentry, and reservations must never become inheritable defaults.
_current_trace_options = ContextVar("baml_python_trace_options", default=())


@contextmanager
def _trace_options_scope(options):
    if options is None:
        yield
        return
    token = _current_trace_options.set(_current_trace_options.get() + (options,))
    try:
        yield
    finally:
        _current_trace_options.reset(token)


class _ExecutionContext:
    __slots__ = ("_state", "_cancel", "_wire", "_unconsumed")

    def __init__(self, frame):
        from collections import Counter
        from .proto import _host_call_handle_keys

        self._state, cancel_bytes = frame
        self._wire = baml_outbound_pb2.BamlOutboundValue.FromString(bytes(cancel_bytes))
        self._unconsumed = Counter(_host_call_handle_keys(self._wire))
        self._cancel = None

    def _release_unconsumed(self):
        from .baml_py import _release_wire_handle

        owned = getattr(self, "_unconsumed", {})
        self._unconsumed = {}
        for key, count in owned.items():
            for _ in range(count):
                _release_wire_handle(key)

    def __del__(self):
        self._release_unconsumed()

    @property
    def cancel(self):
        from .proto import decode_value
        from .typemap import get_type_map

        if self._cancel is None:
            # Primitive-only bridge callers do not need a generated typemap.
            # Public facade access resolves the SDK's registered token class.
            if self._wire is None:
                raise RuntimeError("effective cancellation token could not be decoded")
            wire, self._wire = self._wire, None
            try:
                self._cancel = decode_value(wire, get_type_map(), self._unconsumed)
            finally:
                self._release_unconsumed()
        return self._cancel

    def _key_for_invocation(self):
        return self._state._key_for_invocation()


def current():
    return _current_execution_context.get()


def current_context():
    from .proto import decode_value
    from .typemap import get_type_map

    active = current()
    value = baml_outbound_pb2.BamlOutboundValue.FromString(
        bytes(_invocation_context(active._state if active is not None else None))
    )
    return decode_value(value, get_type_map())


def current_cancel_token():
    """Return the effective token, or None outside a BAML execution context."""
    active = current()
    return active.cancel if active is not None else None
