"""Protobuf encoder/decoder for the BAML bridge_ctypes protocol.

Inbound (09d): Python value → `baml_inbound.proto`. The encoder dispatches
on the Python runtime shape — never on the declared BAML parameter type —
per 09d §1. Rust re-runs BAML type checking after deserializing, so
structural mismatches surface as `CallAck.error` → `BamlError`.

Outbound (09e): `baml_outbound.proto` → Python value. Decoding is driven
by the FQN metadata embedded on `BamlValueClass` / `BamlValueEnum` and
the `handle_type` enum on `BamlHandle`; the caller's declared Python
return type plays no runtime role.
"""

from __future__ import annotations

import enum
import importlib
from typing import Any, Dict, Optional

import pydantic

from .cffi.v1 import baml_inbound_pb2, baml_outbound_pb2
from .baml_core import BamlHandle
from .errors import BamlError


# ---------------------------------------------------------------------------
# Encoding: Python kwargs → CallFunctionArgs (09d §2)
# ---------------------------------------------------------------------------


def _handle_from_handle_backed(value: Any) -> Optional[BamlHandle]:
    """If `value` is a handle-backed Pydantic class (carries a `_handle:
    BamlHandle` PrivateAttr), return that handle; else None.

    Detection is structural: any Pydantic model with a `_handle` attribute
    isinstance-ing to `BamlHandle` qualifies. Keeps the encoder agnostic to
    the concrete stdlib class list — `Image`, `Audio`, `File`, etc.
    """
    handle = getattr(value, "_handle", None)
    if isinstance(handle, BamlHandle):
        return handle
    return None


def _derive_baml_fqn(cls: type) -> str:
    """Reverse 09b §1 routing to produce a BAML FQN for a Python class.

    Informational only — Rust uses it for diagnostics, not correctness
    (09d §2). Returns the empty string when the class is not routed under
    `sdk_root` (user-defined models, runtime not initialized, etc.).
    """
    sdk_root = _safe_sdk_root()
    if not sdk_root:
        return ""
    module = getattr(cls, "__module__", "") or ""
    name = getattr(cls, "__name__", "") or ""
    if not name:
        return ""

    if module == sdk_root:
        subpath = name
    else:
        prefix = sdk_root + "."
        if not module.startswith(prefix):
            return ""
        subpath = f"{module[len(prefix):]}.{name}"

    return _subpath_to_baml_fqn(subpath)


def _subpath_to_baml_fqn(subpath: str) -> str:
    """Reverse the §1 routing table, recursing on the `stream_types.*`
    prefix to handle nested stream companions.
    """
    if subpath.startswith("stream_types."):
        inner = _subpath_to_baml_fqn(subpath[len("stream_types."):])
        return f"{inner}$stream" if inner else ""
    if subpath.startswith("vendor."):
        return subpath[len("vendor."):]
    if subpath.startswith("baml."):
        return subpath
    return f"root.{subpath}"


def _safe_sdk_root() -> str:
    """Fetch `sdk_root` without raising if the runtime is uninitialized.

    FQN derivation is diagnostic, so it must not promote a missing runtime
    into a hard failure on the inbound path.
    """
    try:
        from . import get_runtime  # local import: avoids circular binding at module load
        return get_runtime()._sdk_root or ""
    except Exception:
        return ""


def _set_inbound_value(inbound_value, value: Any, *, kwarg_name: str) -> None:
    """Populate an `InboundValue` oneof from a Python value per 09d §2.

    `kwarg_name` is threaded through so the `TypeError` raised on
    unsupported inputs names the offending top-level kwarg, not the
    nested field we happen to have descended into.
    """
    if value is None:
        return  # oneof unset ≡ null

    # bool must precede int — bool is an int subclass in Python.
    if isinstance(value, bool):
        inbound_value.bool_value = value
        return
    if isinstance(value, int):
        inbound_value.int_value = value
        return
    if isinstance(value, float):
        inbound_value.float_value = value
        return
    if isinstance(value, str):
        inbound_value.string_value = value
        return
    if isinstance(value, (bytes, bytearray)):
        inbound_value.uint8array_value = bytes(value)
        return
    if isinstance(value, (list, tuple)):
        list_val = inbound_value.list_value
        for item in value:
            _set_inbound_value(list_val.values.add(), item, kwarg_name=kwarg_name)
        return
    if isinstance(value, dict):
        map_val = inbound_value.map_value
        for k, v in value.items():
            _set_inbound_map_entry(map_val.entries.add(), k, v, kwarg_name=kwarg_name)
        return
    if isinstance(value, enum.Enum):
        ev = inbound_value.enum_value
        ev.name = _derive_baml_fqn(type(value))
        ev.value = value.name
        return

    # Handle-backed Pydantic classes (Image/Audio/Video/Pdf/File, …) must be
    # checked before the generic `BaseModel` branch — they carry a real
    # `_handle` we want to send verbatim instead of the Pydantic shell.
    handle = _handle_from_handle_backed(value)
    if handle is not None:
        _copy_handle(inbound_value.handle, handle)
        return

    if isinstance(value, pydantic.BaseModel):
        cv = inbound_value.class_value
        cv.name = _derive_baml_fqn(type(value))
        field_dict = value.model_dump()
        for k, v in field_dict.items():
            _set_inbound_map_entry(cv.fields.add(), k, v, kwarg_name=kwarg_name)
        return

    if isinstance(value, BamlHandle):
        _copy_handle(inbound_value.handle, value)
        return

    # UnknownHandle composes a BamlHandle; pull it out.
    from . import UnknownHandle  # local import: defined in __init__.py
    if isinstance(value, UnknownHandle):
        _copy_handle(inbound_value.handle, value._handle)
        return

    raise TypeError(
        f"Cannot encode argument {kwarg_name!r} of type "
        f"{type(value).__name__} into baml_inbound.proto"
    )


def _copy_handle(dst, src: BamlHandle) -> None:
    dst.key = src.key
    dst.handle_type = src.handle_type


def _set_inbound_map_entry(entry, key: Any, value: Any, *, kwarg_name: str) -> None:
    """Populate an `InboundMapEntry` from a (key, value) pair. Key-oneof
    dispatch follows 09d §2 "Map keys"; `bool` precedes `int` (subclass)."""
    if isinstance(key, bool):
        entry.bool_key = key
    elif isinstance(key, str):
        entry.string_key = key
    elif isinstance(key, int):
        entry.int_key = key
    elif isinstance(key, enum.Enum):
        ek = entry.enum_key
        ek.name = _derive_baml_fqn(type(key))
        ek.value = key.name
    else:
        entry.string_key = str(key)  # best-effort fallback
    _set_inbound_value(entry.value, value, kwarg_name=kwarg_name)


def encode_call_args(kwargs: Dict[str, Any]) -> bytes:
    """Encode function keyword arguments as `CallFunctionArgs` protobuf."""
    args = baml_inbound_pb2.CallFunctionArgs()
    for key, value in kwargs.items():
        _set_inbound_map_entry(args.kwargs.add(), key, value, kwarg_name=key)
    return args.SerializeToString()


# ---------------------------------------------------------------------------
# Decoding: BamlOutboundValue → Python values (09e §3)
# ---------------------------------------------------------------------------


def _decode_class(class_value) -> Any:
    """Resolve a `BamlValueClass` to a typed Pydantic model instance.

    Children are decoded first, so `model_validate` receives an
    already-typed field dict — validation mostly acts as a shape check.
    """
    field_dict = {
        entry.key: _decode_value_holder(entry.value)
        for entry in class_value.fields
    }
    # Engine emits `user.*` FQNs; 09b §1 routing lives in `root.*` space.
    # Translate before resolving so `_resolve_type` stays spec-pure.
    from . import _engine_fqn_to_baml_fqn, _resolve_type
    fqn = _engine_fqn_to_baml_fqn(class_value.name.name)
    cls = _resolve_type(fqn)

    # Handle-backed classes never arrive via `class_value` — BEX emits
    # them through `handle_value`. If one slipped through, it's a bug in
    # Rust-side encoding or a stale codegen; fail loud rather than build
    # a half-initialized instance.
    try:
        fields = cls.model_fields  # type: ignore[attr-defined]
    except AttributeError:
        fields = {}
    if "_handle" in getattr(cls, "__private_attributes__", {}):
        raise BamlError(
            f"BEX emitted class_value for handle-backed class {fqn!r}"
        )

    if issubclass(cls, pydantic.BaseModel):
        return cls.model_validate(field_dict)
    # Not a BaseModel — shouldn't happen for a well-formed SDK; fall back
    # to a plain dict so callers aren't silently lied to.
    return field_dict


def _decode_enum(enum_value) -> Any:
    """Resolve a `BamlValueEnum` to a member of the generated enum class."""
    variant = enum_value.value
    from . import _engine_fqn_to_baml_fqn, _resolve_type
    fqn = _engine_fqn_to_baml_fqn(enum_value.name.name)
    cls = _resolve_type(fqn)
    try:
        return cls(variant)
    except ValueError as exc:
        raise BamlError(
            f"BEX returned variant {variant!r} that does not name a "
            f"member of {fqn!r}"
        ) from exc


def _decode_handle(handle) -> Any:
    """Map `(key, handle_type)` to a Python object per 09e §3.

    Known stdlib handle classes (`Image`, `Audio`, …, `File`) live under
    `<sdk_root>.baml.media.*` / `<sdk_root>.baml.io.*`. Those classes
    don't exist yet (phase 6 codegen lands them), so for now we resolve
    `HANDLE_UNKNOWN` and the opaque variants and let the rest surface as
    `BamlError` — BEX shouldn't emit them until the stdlib is wired.
    """
    from . import UnknownHandle  # local import: defined in __init__.py
    HT = baml_inbound_pb2.BamlHandleType
    wrapped = BamlHandle(handle.key, handle.handle_type)

    ht = handle.handle_type
    if ht == HT.HANDLE_UNKNOWN:
        return UnknownHandle(wrapped)
    # Opaque-to-Python ADTs — surface as `UnknownHandle` per 09e §3.
    if ht in (HT.ADT_PROMPT_AST, HT.ADT_COLLECTOR, HT.ADT_TYPE):
        return UnknownHandle(wrapped)
    if ht == HT.HANDLE_UNSPECIFIED:
        raise BamlError("BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)")
    if ht == HT.FUNCTION_REF:
        raise BamlError("Function-ref handles do not cross FFI today")
    if ht == HT.ADT_MEDIA_GENERIC:
        raise BamlError("Generic media has no stdlib class today")

    # Named stdlib handle classes — resolve lazily against sdk_root.
    # Each class exposes a `__handle`-keyword constructor (09e §3).
    subpath = _HANDLE_TYPE_SUBPATHS.get(ht)
    if subpath is None:
        raise BamlError(f"Unknown BamlHandleType {ht!r}")
    cls = _resolve_under_sdk_root(subpath)
    if cls is None:
        raise BamlError(
            f"BEX returned handle_type {ht!r} but {subpath!r} is not "
            f"defined under sdk_root"
        )
    return cls(__handle=wrapped)


_HANDLE_TYPE_SUBPATHS = {
    baml_inbound_pb2.BamlHandleType.ADT_MEDIA_IMAGE: "baml.media.Image",
    baml_inbound_pb2.BamlHandleType.ADT_MEDIA_AUDIO: "baml.media.Audio",
    baml_inbound_pb2.BamlHandleType.ADT_MEDIA_VIDEO: "baml.media.Video",
    baml_inbound_pb2.BamlHandleType.ADT_MEDIA_PDF: "baml.media.Pdf",
    baml_inbound_pb2.BamlHandleType.RESOURCE_FILE: "baml.io.File",
    baml_inbound_pb2.BamlHandleType.RESOURCE_SOCKET: "baml.net.Socket",
    baml_inbound_pb2.BamlHandleType.RESOURCE_HTTP_RESPONSE: "baml.http.Response",
}


def _resolve_under_sdk_root(subpath: str) -> Optional[type]:
    """Import `<sdk_root>.<subpath>` and return the trailing attr, or None
    if any step fails. Used for stdlib handle classes that may not yet be
    wired in the sim tree."""
    sdk_root = _safe_sdk_root()
    if not sdk_root:
        return None
    full = f"{sdk_root}.{subpath}"
    module_path, _, type_name = full.rpartition(".")
    try:
        module = importlib.import_module(module_path)
    except ImportError:
        return None
    return getattr(module, type_name, None)


def _decode_literal(literal) -> Any:
    which = literal.WhichOneof("literal")
    if which == "string_literal":
        return literal.string_literal.value
    if which == "int_literal":
        return literal.int_literal.value
    if which == "bool_literal":
        return literal.bool_literal.value
    return None


def _decode_value_holder(holder) -> Any:
    """Convert a `BamlOutboundValue` message to a typed Python value."""
    which = holder.WhichOneof("value")
    if which is None or which == "null_value":
        return None
    if which == "string_value":
        return holder.string_value
    if which == "int_value":
        return holder.int_value
    if which == "float_value":
        return holder.float_value
    if which == "bool_value":
        return holder.bool_value
    if which == "uint8array_value":
        return holder.uint8array_value
    if which == "literal_value":
        return _decode_literal(holder.literal_value)
    if which == "list_value":
        return [_decode_value_holder(item) for item in holder.list_value.items]
    if which == "map_value":
        return {
            entry.key: _decode_value_holder(entry.value)
            for entry in holder.map_value.entries
        }
    if which == "class_value":
        return _decode_class(holder.class_value)
    if which == "enum_value":
        return _decode_enum(holder.enum_value)
    if which == "union_variant_value":
        # Union metadata is discarded — Python is duck-typed. The inner
        # value self-describes (09e §3).
        return _decode_value_holder(holder.union_variant_value.value)
    if which == "handle_value":
        return _decode_handle(holder.handle_value)
    if which in ("media_value", "prompt_ast_value"):
        raise BamlError(
            f"BEX emitted {which!r} on the FFI path — media/prompt AST "
            f"are expected via handle_value, not inline"
        )
    return None


def decode_call_result(data: bytes) -> Any:
    """Decode a `BamlOutboundValue` protobuf to a Python value."""
    holder = baml_outbound_pb2.BamlOutboundValue()
    holder.ParseFromString(data)
    return _decode_value_holder(holder)
