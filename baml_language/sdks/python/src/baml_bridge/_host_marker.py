"""Exact wrapper identity and a one-shot permit scoped to callback execution."""

from contextvars import ContextVar
from weakref import WeakKeyDictionary
from types import FunctionType, MethodType

_markers = WeakKeyDictionary()
_adoption = ContextVar("baml_host_adoption", default=None)


def marker_for(function):
    # Never inspect __wrapped__ or copied function attributes for adoption.
    if type(function) is MethodType:
        function = function.__func__
    return _markers.get(function) if type(function) is FunctionType else None


def register_marker(function, marker):
    _markers[function] = marker


def consume_adoption(marker):
    permit = _adoption.get()
    if permit is None or permit[0] is not marker or permit[1]:
        return False
    permit[1] = True
    return True
