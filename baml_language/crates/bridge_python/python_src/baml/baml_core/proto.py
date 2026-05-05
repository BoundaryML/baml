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
import typing
from typing import Any, Dict, List, Optional

import pydantic

from baml.cffi.v1 import baml_inbound_pb2, baml_outbound_pb2
from .baml_py import BamlAudio, BamlHandle, BamlImage, BamlPdf, BamlVideo
from .errors import BamlError


# Media PyO3 types live behind their Rust class names; the inbound encoder
# wraps each in an `InboundClassValue` whose `name` field carries the BAML
# stdlib FQN (15b §line 21).
_MEDIA_CLASS_TO_FQN: Dict[type, str] = {
    BamlImage: "baml.media.Image",
    BamlAudio: "baml.media.Audio",
    BamlVideo: "baml.media.Video",
    BamlPdf: "baml.media.Pdf",
}
_MEDIA_PYO3_TYPES = tuple(_MEDIA_CLASS_TO_FQN.keys())


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


def _base_class_for_fqn(cls: type) -> type:
    """Return the unparameterized origin of a Pydantic generic class.

    `Box[int]` is a runtime subclass produced by Pydantic v2's
    `__class_getitem__`; its FQN on the wire is the base `Box`'s, not the
    parameterization (`13b` §2.1). For non-generic classes this is a
    no-op — `cls` already names the class we want.
    """
    meta = getattr(cls, "__pydantic_generic_metadata__", None)
    if meta and meta.get("origin"):
        return meta["origin"]
    return cls


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

    Phase 12a collapsed the BEP-030 `root.*` spec convention into the
    engine's `user.*` package convention; everything downstream (engine
    `resolved_class_names`, `lookup_function`, outbound `_resolve_type`)
    now expects `user.*` end-to-end, so emit it directly here. The
    project-boundary coercion in `bex_project::bex::coerce_arg_to_declared_type`
    only rewrites the top-level arg's class name; nested classes,
    enums, and dict/list element types must arrive with the engine FQN
    already on them or the engine panics on lookup.
    """
    if subpath.startswith("stream_types."):
        inner = _subpath_to_baml_fqn(subpath[len("stream_types."):])
        return f"{inner}$stream" if inner else ""
    if subpath.startswith("vendor."):
        return subpath[len("vendor."):]
    if subpath.startswith("baml."):
        return subpath
    return f"user.{subpath}"


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
        ev.name = _derive_baml_fqn(_base_class_for_fqn(type(value)))
        ev.value = value.name
        return

    # Media PyO3 types — wrap into an `InboundClassValue` per 15b. The
    # encoder borrows the inner `Arc<MediaValue>` into the global handle
    # table (via `_insert_into_handle_table()`), then emits a
    # class-shaped inbound where the only field is `_data` carrying that
    # handle. The engine's `convert_external_to_vm_value` looks up the
    # class by qualified name (`baml.media.Pdf` etc.) and lowers `_data`
    # back into a VM-side `RustData` wrapping the same Arc.
    #
    # The handle-table slot is owned by the engine for the duration of
    # the call — once `convert_external_to_vm_value` allocates a
    # `RustData` on the VM heap (which holds its own `Arc::clone`), the
    # slot can be released.
    #
    # Note: BamlHandle is bypassed entirely on the media path. The PyO3
    # media classes write directly into HANDLE_TABLE and expose the raw
    # u64 key + statically-known handle_type tag for the proto.
    if isinstance(value, _MEDIA_PYO3_TYPES):
        cv = inbound_value.class_value
        cv.name = _MEDIA_CLASS_TO_FQN[type(value)]
        data_entry = cv.fields.add()
        data_entry.string_key = "_data"
        handle_proto = data_entry.value.handle
        handle_proto.key = value._insert_into_handle_table()
        handle_proto.handle_type = type(value)._handle_type()
        return

    # Handle-backed Pydantic classes (`baml.io.File`, `baml.net.Socket`,
    # …) must be checked before the generic `BaseModel` branch — they
    # carry a real `_handle` we want to send verbatim instead of the
    # Pydantic shell. Media classes (Image/Audio/Video/Pdf) intentionally
    # don't carry a `_handle` PrivateAttr after 15d M2 — they're PyO3
    # types holding `Arc<MediaValue>` directly and take the
    # media-specific branch above.
    handle = _handle_from_handle_backed(value)
    if handle is not None:
        _copy_handle(inbound_value.handle, handle)
        return

    if isinstance(value, pydantic.BaseModel):
        cv = inbound_value.class_value
        # Pydantic generic subclasses (`Box[int]`) keep `__module__` from
        # the base, but we still want the *base* `Box`'s FQN on the wire —
        # `13b` §2.1. The Rust-side type checker already knows the
        # declared parameter type from the function signature.
        cv.name = _derive_baml_fqn(_base_class_for_fqn(type(value)))
        # Walk fields by attribute access (Pydantic v2's `__iter__`
        # yields `(name, value)` without recursive serialization).
        # `model_dump()` would flatten nested Pydantic instances into
        # dicts and lose the type info — the Rust-side coercer would
        # then see them as `Map` instead of `Instance`, so a
        # `Box<Box<int>>` round-trip collapses into bare dicts at the
        # second level.
        for k, v in dict(value).items():
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


_MEDIA_KIND_SUBPATHS = {
    baml_outbound_pb2.MediaTypeEnum.IMAGE: "baml.media.Image",
    baml_outbound_pb2.MediaTypeEnum.AUDIO: "baml.media.Audio",
    baml_outbound_pb2.MediaTypeEnum.VIDEO: "baml.media.Video",
    baml_outbound_pb2.MediaTypeEnum.PDF: "baml.media.Pdf",
}


def _baml_ty_to_python_type(baml_ty) -> Any:
    """Walk a `BamlTy` proto and return the corresponding Python type.

    Runtime mirror of codegen-time `translate_ty` (`13a` §3): same BAML→
    Python mapping, but produces a `type` object (used for
    `cls[args]` parameterization) rather than a source string. The two
    share the mapping table — keep them in sync when adding new
    `BamlTy` variants. See `13b` §3.3.
    """
    which = baml_ty.WhichOneof("type")
    if which is None:
        return typing.Any
    if which == "string_type":
        return str
    if which == "int_type":
        return int
    if which == "float_type":
        return float
    if which == "bool_type":
        return bool
    if which == "null_type":
        return type(None)
    if which == "uint8array_type":
        return bytes
    if which == "any_type" or which == "unknown_type":
        return typing.Any
    if which == "literal_type":
        inner = _decode_literal(baml_ty.literal_type)
        return typing.Literal[inner]  # type: ignore[valid-type]
    if which == "media_type":
        subpath = _MEDIA_KIND_SUBPATHS.get(baml_ty.media_type.media)
        if subpath is None:
            return typing.Any
        cls = _resolve_under_sdk_root(subpath)
        return cls if cls is not None else typing.Any
    if which == "class_type":
        from . import _resolve_type
        cls = _resolve_type(baml_ty.class_type.name.name)
        return _parameterize(cls, baml_ty.class_type.name.generic_args)
    if which == "enum_type":
        from . import _resolve_type
        return _resolve_type(baml_ty.enum_type.name)
    if which == "type_alias_type":
        from . import _resolve_type
        cls = _resolve_type(baml_ty.type_alias_type.name.name)
        return _parameterize(cls, baml_ty.type_alias_type.name.generic_args)
    if which == "list_type":
        return List[_baml_ty_to_python_type(baml_ty.list_type.item_type)]  # type: ignore[valid-type]
    if which == "map_type":
        return Dict[  # type: ignore[valid-type]
            _baml_ty_to_python_type(baml_ty.map_type.key_type),
            _baml_ty_to_python_type(baml_ty.map_type.value_type),
        ]
    if which == "optional_type":
        return Optional[_baml_ty_to_python_type(baml_ty.optional_type.value)]  # type: ignore[valid-type]
    if which == "union_variant_type":
        from . import _resolve_type
        cls = _resolve_type(baml_ty.union_variant_type.name.name)
        return _parameterize(cls, baml_ty.union_variant_type.name.generic_args)
    raise BamlError(f"Unsupported BamlTy variant {which!r} in generic arg")


def _parameterize(cls, generic_args):
    """Apply BAML generic args to a symbol via `cls[arg_types…]`.

    Works for any subscriptable symbol — Pydantic generics, generic
    type aliases (PEP 695 `TypeAliasType` or `typing` aliases like
    `List[T]`), `typing.Generic` subclasses, etc. The try/except
    catches the failure modes (arity mismatch, non-generic class,
    fully-bound alias) and falls back to the unparameterized `cls`.

    No-op when `generic_args` is empty (the rollout-safe path: works
    before the Rust producer is updated to populate them — `13b` §3.5).
    """
    if not generic_args:
        return cls
    py_args = tuple(_baml_ty_to_python_type(g.ty) for g in generic_args)
    try:
        if len(py_args) == 1:
            return cls[py_args[0]]
        return cls[py_args]
    except (TypeError, AttributeError):
        return cls


def _decode_class(class_value) -> Any:
    """Resolve a `BamlValueClass` to a typed Pydantic model instance.

    Children are decoded first, so `model_validate` receives an
    already-typed field dict — validation mostly acts as a shape check.
    Generic classes parameterize before validation so static-checker
    annotations (`Box[int]`) line up with the runtime instance —
    `13b` §3.4.
    """
    field_dict = {
        entry.key: _decode_value_holder(entry.value)
        for entry in class_value.fields
    }
    # Emit always fully qualifies, so the engine FQN already matches
    # what `_resolve_type` consumes (`12a-namespace-rules.md §5`).
    from . import _resolve_type
    fqn = class_value.name.name
    cls = _resolve_type(fqn)

    # Media stdlib classes (`baml.media.*`) are PyO3 types holding
    # `Arc<MediaValue>`. The engine emits them as
    # `class_value { name: "baml.media.Pdf", fields: { _data: handle_value }}`
    # (its on-wire shape mirrors the BAML class definition); the inner
    # `_data` decode already constructed a fresh `BamlPdf` via
    # `_decode_handle` → `cls._take_from_handle_table(...)`. Unwrap and
    # return it directly — `BamlPdf` is not a Pydantic model and has no
    # `model_validate`.
    if cls in _MEDIA_PYO3_TYPES and "_data" in field_dict:
        return field_dict["_data"]

    parameterized = _parameterize(cls, class_value.name.generic_args)
    if issubclass(cls, pydantic.BaseModel):
        return parameterized.model_validate(field_dict)
    # Not a BaseModel — shouldn't happen for a well-formed SDK; fall back
    # to a plain dict so callers aren't silently lied to.
    return field_dict


def _decode_enum(enum_value) -> Any:
    """Resolve a `BamlValueEnum` to a member of the generated enum class."""
    variant = enum_value.value
    from . import _resolve_type
    fqn = enum_value.name.name
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

    Media handle types bypass `BamlHandle` entirely — the PyO3 media
    classes resolve the raw key against `HANDLE_TABLE` themselves via
    `_take_from_handle_table`. Other handle-backed stdlib classes
    (File/Socket/Response) and the `UnknownHandle` fallback still take a
    `BamlHandle` instance until that class is removed in a later phase.
    """
    from . import UnknownHandle  # local import: defined in __init__.py
    HT = baml_inbound_pb2.BamlHandleType
    ht = handle.handle_type

    # Media stdlib classes — resolve the raw key directly. We do this
    # before constructing a BamlHandle so the media path never touches
    # that wrapper.
    if ht in (
        HT.ADT_MEDIA_IMAGE,
        HT.ADT_MEDIA_AUDIO,
        HT.ADT_MEDIA_VIDEO,
        HT.ADT_MEDIA_PDF,
    ):
        subpath = _HANDLE_TYPE_SUBPATHS[ht]
        cls = _resolve_under_sdk_root(subpath)
        if cls is None:
            raise BamlError(
                f"BEX returned handle_type {ht!r} but {subpath!r} is not "
                f"defined under sdk_root"
            )
        return cls._take_from_handle_table(handle.key)

    wrapped = BamlHandle(handle.key, handle.handle_type)

    if ht in (HT.UNTAGGED_RUST_DATA, HT.UNTAGGED_BEX_HEAP):
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
