"""Bounded host observations. Builtin adapters read stored data, not serializers."""

from contextvars import ContextVar
from datetime import date, datetime, timezone
from enum import Enum
import inspect
import json
import math
import threading
import types
import typing
import weakref

from .typemap import get_type_map

try:
    from pydantic import BaseModel
    from pydantic.fields import FieldInfo
except ImportError:
    BaseModel = FieldInfo = None

try:
    from zoneinfo import ZoneInfo
except ImportError:
    ZoneInfo = None

_active = ContextVar("baml_host_capture_active", default=None)
_lock = threading.RLock()
_handlers = {}


def register_capture(value_type, handler):
    """Register a synchronous fallback projection for a type and its subclasses.

    Native BAML types and builtin scalars/containers take precedence. Projections
    recursively use the same capture policy; errors and cycles stay opaque.
    """
    if not isinstance(value_type, type) or not callable(handler):
        raise TypeError("register_capture expects a type and a synchronous callable")
    if (
        inspect.iscoroutinefunction(handler)
        or inspect.isgeneratorfunction(handler)
        or inspect.isasyncgenfunction(handler)
    ):
        raise TypeError("capture projections must be synchronous ordinary callables")
    identity = id(value_type)

    def discard(reference):
        with _lock:
            if _handlers.get(identity, (None,))[0] is reference:
                _handlers.pop(identity, None)

    with _lock:
        _handlers[identity] = (weakref.ref(value_type, discard), handler)
    return handler


def capture_for(value_type):
    """Decorator spelling for register_capture(type, function)."""
    return lambda handler: register_capture(value_type, handler)


def _attributes(cls):
    return type.__dict__["__dict__"].__get__(cls)


def _mro(cls):
    return type.__dict__["__mro__"].__get__(cls)


def _inherits(mro, cls):
    # Identity only: `cls in mro` would call an application metaclass's __eq__.
    return any(base is cls for base in mro)


def _class_value(cls, name):
    for base in _mro(cls):
        attributes = _attributes(base)
        if name in attributes:
            return attributes[name]
    return None


def _diagnostic():
    from ._instrumentation import _diagnostic

    _diagnostic("host capture projection failed")


def capture(value):
    """Return copied tagged data for the native bridge, never original objects."""
    inherited = _active.get()
    if inherited is not None and inherited[0]:
        return '["unavailable"]'
    guard = [True]
    token = _active.set(guard)
    with _lock:
        handlers = dict(_handlers)
    remaining = 512
    bytes_left = 64 * 1024
    active = set()

    def text(value):
        """Charge `value`'s UTF-8 size; None when it has no UTF-8 form."""
        nonlocal bytes_left
        if type(value) is not str or len(value) > bytes_left:
            return False
        try:
            size = len(value.encode("utf-8"))
        except UnicodeEncodeError:
            # A lone surrogate (e.g. a surrogateescape file name) would make the
            # whole observation undecodable by the native bridge.
            return None
        if size > bytes_left:
            return False
        bytes_left -= size
        return True

    def native_type(annotation, depth):
        nonlocal remaining
        if depth > 8 or remaining <= 0:
            return ["unknown"]
        remaining -= 1
        for cls, label in (
            (int, "int"),
            (float, "float"),
            (str, "string"),
            (bool, "bool"),
            (type(None), "null"),
        ):
            if annotation is cls:
                return [label]
        if isinstance(annotation, type):
            native = get_type_map().capture_type(annotation)
            if native and text(native[0]):
                if native[1]:
                    return ["enum", native[0]]
                metadata = _class_value(annotation, "__pydantic_generic_metadata__")
                args = metadata.get("args", ()) if type(metadata) is dict else ()
                return [
                    "class",
                    native[0],
                    [native_type(arg, depth + 1) for arg in args],
                ]
            return ["unknown"]
        # Only typing's own annotation objects are inspected for compositions.
        if (
            type(annotation) is types.GenericAlias
            or _attributes(type(annotation)).get("__module__") == "typing"
        ):
            origin, args = typing.get_origin(annotation), typing.get_args(annotation)
            if origin is list and len(args) == 1:
                return ["list", native_type(args[0], depth + 1)]
            if origin is dict and len(args) == 2:
                return [
                    "map",
                    native_type(args[0], depth + 1),
                    native_type(args[1], depth + 1),
                ]
            if origin is typing.Union:
                return ["union", [native_type(arg, depth + 1) for arg in args]]
        return ["unknown"]

    def model_fields(value, cls, depth):
        fields = _class_value(cls, "__pydantic_fields__")
        stored = BaseModel.__dict__["__dict__"].__get__(value)
        if type(fields) is not dict or type(stored) is not dict:
            return None
        if len(fields) > remaining:
            return None
        result = []
        for name, field in dict.items(fields):
            if type(name) is not str:
                return None
            alias = None
            if type(field) is FieldInfo:
                alias = FieldInfo.__dict__["serialization_alias"].__get__(field)
                if alias is None:
                    alias = FieldInfo.__dict__["alias"].__get__(field)
            key = alias if type(alias) is str else name
            if not text(key):
                return None
            result.append(
                [
                    key,
                    copy(dict.get(stored, name), depth + 1)
                    if name in stored
                    else opaque(),
                ]
            )
        return result

    def opaque():
        nonlocal remaining
        if remaining <= 0:
            return ["values"]
        remaining -= 1
        return ["unavailable"]

    def copy(value, depth):
        nonlocal remaining
        # Markers count too: the native decoder charges every node it reads.
        if remaining <= 0:
            return ["values"]
        remaining -= 1
        if depth > 8:
            return ["depth"]
        cls = type(value)
        if value is None:
            return ["null"]
        if cls is bool:
            return ["bool", value]
        if cls is int:
            return (
                ["number", value]
                if -(1 << 63) <= value < (1 << 63)
                else ["unavailable"]
            )
        if cls is float:
            return ["number", value] if math.isfinite(value) else ["unavailable"]
        if cls is str:
            fits = text(value)
            if fits is None:
                return ["unavailable"]
            return ["string", value] if fits else ["bytes"]
        identity = id(value)
        if identity in active:
            return ["unavailable"]
        active.add(identity)
        try:
            if cls is list or cls is tuple:
                if len(value) > remaining:
                    return ["values"]
                return ["list", [copy(item, depth + 1) for item in value]]
            if cls is dict:
                if len(value) > remaining:
                    return ["values"]
                entries = []
                for key, item in dict.items(value):
                    if type(key) is not str:
                        return ["unavailable"]
                    fits = text(key)
                    if fits is None:
                        return ["unavailable"]
                    if not fits:
                        return ["bytes"]
                    entries.append([key, copy(item, depth + 1)])
                return ["map", entries]
            mro = _mro(cls)
            native = get_type_map().capture_type(cls)
            if native is not None:
                name, is_enum, declared = native
                if not text(name):
                    return ["bytes"]
                if is_enum and _inherits(mro, Enum):
                    stored = Enum.__dict__["__dict__"].__get__(value)
                    # The value is the BAML variant; the member name can be
                    # renamed, e.g. `None_` for BAML's `None`.
                    variant = dict.get(stored, "_value_")
                    return ["enum", name, variant] if text(variant) else ["unavailable"]
                if BaseModel is not None and _inherits(mro, BaseModel):
                    fields = model_fields(value, declared, depth)
                    metadata = _class_value(cls, "__pydantic_generic_metadata__")
                    args = metadata.get("args", ()) if type(metadata) is dict else ()
                    return (
                        [
                            "class",
                            name,
                            fields,
                            [native_type(arg, depth + 1) for arg in args],
                        ]
                        if fields is not None
                        else ["values"]
                    )
            for base in mro:
                entry = handlers.get(id(base))
                if entry is not None and entry[0]() is base:
                    return copy(entry[1](value), depth + 1)
            if BaseModel is not None and _inherits(mro, BaseModel):
                fields = model_fields(value, cls, depth)
                return ["map", fields] if fields is not None else ["values"]
            if _inherits(mro, datetime):
                zone = datetime.__dict__["tzinfo"].__get__(value)
                if (
                    zone is not None
                    and type(zone) is not timezone
                    and (ZoneInfo is None or type(zone) is not ZoneInfo)
                ):
                    return ["unavailable"]
                return copy(datetime.isoformat(value), depth)
            if _inherits(mro, date):
                return copy(date.isoformat(value), depth)
            if _inherits(mro, BaseException):
                name = type.__dict__["__qualname__"].__get__(cls)
                args = BaseException.__dict__["args"].__get__(value)
                return copy({"type": name, "args": args}, depth)
            return ["unavailable"]
        except Exception:
            _diagnostic()
            return ["unavailable"]
        finally:
            active.remove(identity)

    try:
        return json.dumps(copy(value, 0), ensure_ascii=True, separators=(",", ":"))
    finally:
        guard[0] = False
        _active.reset(token)
