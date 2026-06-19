"""Protobuf encoder/decoder for the BAML bridge_ctypes protocol.

Inbound (09d): Python value → `baml_inbound.proto`. The encoder dispatches
on the Python runtime shape — never on the declared BAML parameter type —
per 09d §1. Rust re-runs BAML type checking after deserializing, so
structural mismatches surface as `CallAck.error` → `BamlError`.

Outbound (09e): `baml_outbound.proto` → Python value. Decoding is driven
by the FQN metadata embedded on `BamlValueClass` / `BamlValueEnum` and
the wire `BamlHandle.handle_type` tag (read by `_decode_handle`); the
caller's declared Python return type plays no runtime role.
"""

from __future__ import annotations

import enum
import os
import typing
from typing import Any, Dict, List, Optional

from .cffi.v1 import baml_inbound_pb2, baml_outbound_pb2
from .baml_py import (
    BamlAudio,
    BamlImage,
    BamlPdf,
    BamlPyHandle,
    BamlVideo,
    register_host_callable,
    release_host_callable,
)
from ._stream import BamlStream
from .errors import BamlError, BamlPanic, attach_baml_traceback
from .typemap import BamlTypeMap, get_type_map

def _is_pydantic_model(value: Any) -> bool:
    try:
        from pydantic import BaseModel # type: ignore[import-untyped]
    except ImportError:
        return False
    return isinstance(value, BaseModel)


def _is_pydantic_model_class(cls: type) -> bool:
    try:
        from pydantic import BaseModel # type: ignore[import-untyped]
    except ImportError:
        return False
    return issubclass(cls, BaseModel)


# Media PyO3 types — kept as a tuple for `isinstance` dispatch in the
# encoder and `_decode_class` unwrap (15b §line 21). The engine FQN
# for each class is looked up via `get_type_map().py_type_to_baml_type(...)`
# at encode time; the typemap seeds the PyO3 identity → `baml.media.*`
# overrides at construction (25b2 §"reverse map overrides").
_MEDIA_PYO3_TYPES = (BamlImage, BamlAudio, BamlVideo, BamlPdf)


# ---------------------------------------------------------------------------
# Encoding: Python kwargs → CallFunctionArgs (09d §2)
# ---------------------------------------------------------------------------


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


def _set_inbound_value(
    inbound_value: baml_inbound_pb2.InboundValue,
    value: Any,
    *,
    kwarg_name: str,
    registered: Optional[List[int]] = None,
) -> None:
    """Populate an `InboundValue` oneof from a Python value per 09d §2.

    `kwarg_name` is threaded through so the `TypeError` raised on
    unsupported inputs names the offending top-level kwarg, not the
    nested field we happen to have descended into.

    `registered`, when supplied, collects the host-value keys minted by the
    callable branch so the encode path can roll the registrations back if
    encoding fails before the bytes reach the engine. Both the argument path
    (`encode_call_args`) and the host-call *result* encode path (Rust's
    `encode_result_inbound`) supply it: a callable nested in a composite value
    whose encoding then aborts would otherwise leak, since the engine never
    receives — and so never releases — it.
    """
    if value is None:
        return  # oneof unset ≡ null

    # bool must precede int — bool is an int subclass in Python.
    if isinstance(value, bool):
        inbound_value.bool_value = value
        return
    if isinstance(value, int):
        # Python's `int` is arbitrary-precision, but the wire `int_value`
        # field is `int64`. Values outside i64 range overflow protobuf
        # serialization, so route them through `bigint_value` instead.
        # Hex / base sixteen on the wire; `format(value, "x")` preserves a
        # leading minus for negatives.
        if -(1 << 63) <= value < (1 << 63):
            inbound_value.int_value = value
        else:
            inbound_value.bigint_value = format(value, "x")
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
        # Mark the `list_value` oneof arm present even for an empty list.
        # Merely reading `inbound_value.list_value` doesn't set the oneof
        # case; without an `add()` (empty list) the arm stays unset, which
        # the engine reads as null (see `value is None` above) — so an
        # empty list would round-trip back as `None`.
        list_val.SetInParent()
        for item in value:
            _set_inbound_value(
                list_val.values.add(), item, kwarg_name=kwarg_name, registered=registered
            )
        return
    if isinstance(value, dict):
        map_val = inbound_value.map_value
        # Same as the empty-list case above: set the oneof arm so an empty
        # dict encodes as an empty map rather than an unset (null) value.
        map_val.SetInParent()
        for k, v in value.items():
            _set_inbound_map_entry(
                map_val.entries.add(), k, v, kwarg_name=kwarg_name, registered=registered
            )
        return
    if isinstance(value, enum.Enum):
        ev = inbound_value.enum_value
        ev.name = get_type_map().py_type_to_baml_type(_base_class_for_fqn(type(value)))
        ev.value = value.name
        return

    # `BamlPyHandle` is its own top-level inbound variant — peer to
    # `class_value` / `enum_value` / etc. Must precede `pydantic.BaseModel`
    # so a future `BamlPyHandle` subclass would land here, and must
    # precede the media-class branch since the media types compose a
    # `BamlPyHandle` internally and recurse here on `_to_pyhandle()`.
    if isinstance(value, BamlPyHandle):
        key, ht = value._clone_key_for_wire()
        inbound_value.handle.key = key
        # Wire field stays populated for cross-bridge compat. The proto
        # field is typed as the enum class, but `BamlHandleType` is an
        # `int` subclass so the runtime accepts a bare int — cast for
        # the static checker.
        inbound_value.handle.handle_type = typing.cast(
            baml_inbound_pb2.BamlHandleType, ht
        )
        return

    # `BamlStream` (21b §"Phase 4"): lifted to a bare `handle_value` on
    # the wire — the engine intercepts the outer Stream class at
    # `convert_heap_ptr_to_external_with_type` and reconstructs the heap
    # pointer in `convert_external_to_vm_value`'s `Adt(TaggedHeapHandle)`
    # arm. So encode as `handle_value(ADT_TAGGED_HEAP_HANDLE)` rather
    # than the media-style `class_value(name, _data: handle_value)` wrap.
    # Inbound stays a bare `BamlHandle` (key + type only) since the
    # engine's `HANDLE_TABLE` row already carries the receiver's `ty`.
    if isinstance(value, BamlStream):
        return _set_inbound_value(
            inbound_value, value._to_pyhandle(), kwarg_name=kwarg_name, registered=registered
        )

    # Media PyO3 types — wrap into an `InboundClassValue` per 15b. The
    # only field is `_data`, recursively encoded; the recursion lands on
    # the `BamlPyHandle` branch above. The engine FQN comes from the
    # typemap's reverse-map seeded overrides (25b2 §"reverse map").
    if isinstance(value, _MEDIA_PYO3_TYPES):
        cv = inbound_value.class_value
        cv.name = get_type_map().py_type_to_baml_type(type(value))
        data_entry = cv.fields.add()
        data_entry.string_key = "_data"
        _set_inbound_value(
            data_entry.value, value._to_pyhandle(), kwarg_name=kwarg_name, registered=registered
        )
        return

    # Python callables → register in the host-value table and emit a
    # `Handle` with `HOST_VALUE_CALLABLE`. The Rust side decodes this into
    # `BexExternalValue::HostValue` and binds it to an `Object::HostClosure`
    # so BAML code can invoke it directly. Must precede the pydantic-model
    # branch because Pydantic models are sometimes callable (classes); we
    # only register *non-class* callables (functions, lambdas, methods,
    # callable instances). The check order is: Pydantic class instances
    # (which `_is_pydantic_model` returns True for) fall through here,
    # because `isinstance(value, type)` is False; bare classes would not
    # reach this branch since `_is_pydantic_model_class` only accepts
    # already-Pydantic-model classes. For non-class callables, register.
    if callable(value) and not isinstance(value, type) and not _is_pydantic_model(value):
        key = register_host_callable(value)
        # Record the key so the encode path can release it if a later
        # kwarg fails to encode (the call never reaches the engine, so the
        # engine would never decode — and never release — this key).
        if registered is not None:
            registered.append(key)
        inbound_value.handle.key = key
        inbound_value.handle.handle_type = typing.cast(
            baml_inbound_pb2.BamlHandleType,
            baml_inbound_pb2.BamlHandleType.HOST_VALUE_CALLABLE,
        )
        return

    if _is_pydantic_model(value):
        cv = inbound_value.class_value
        # Pydantic generic subclasses (`Box[int]`) keep `__module__` from
        # the base, but we still want the *base* `Box`'s FQN on the wire —
        # `13b` §2.1. The Rust-side type checker already knows the
        # declared parameter type from the function signature.
        cv.name = get_type_map().py_type_to_baml_type(_base_class_for_fqn(type(value)))
        # Walk fields by attribute access (Pydantic v2's `__iter__`
        # yields `(name, value)` without recursive serialization).
        # `model_dump()` would flatten nested Pydantic instances into
        # dicts and lose the type info — the Rust-side coercer would
        # then see them as `Map` instead of `Instance`, so a
        # `Box<Box<int>>` round-trip collapses into bare dicts at the
        # second level.
        for k, v in dict(value).items():
            _set_inbound_map_entry(
                cv.fields.add(), k, v, kwarg_name=kwarg_name, registered=registered
            )
        # Private attrs aren't iterated by `dict(value)`. Codegen emits
        # `$rust_type` fields as private attrs (single-underscore names);
        # walk them explicitly so `BamlPyHandle`-backed shells round-trip.
        # `__pydantic_private__` is None when the model declares no
        # private attrs.
        private = getattr(value, "__pydantic_private__", None) or {}
        for k, v in private.items():
            if isinstance(v, BamlPyHandle):
                _set_inbound_map_entry(
                    cv.fields.add(), k, v, kwarg_name=kwarg_name, registered=registered
                )
        return

    raise TypeError(
        f"Cannot encode argument {kwarg_name!r} of type "
        f"{type(value).__name__} into baml_inbound.proto"
    )


def _set_inbound_map_entry(
    entry, key: Any, value: Any, *, kwarg_name: str, registered: Optional[List[int]] = None
) -> None:
    """Populate an `InboundMapEntry` from a (key, value) pair. Key-oneof
    dispatch follows 09d §2 "Map keys"; `bool` precedes `int` (subclass).

    `registered` is threaded to `_set_inbound_value` for encode-error
    rollback (see that function)."""
    if isinstance(key, bool):
        entry.bool_key = key
    elif isinstance(key, str):
        entry.string_key = key
    elif isinstance(key, int):
        entry.int_key = key
    elif isinstance(key, enum.Enum):
        ek = entry.enum_key
        ek.name = get_type_map().py_type_to_baml_type(type(key))
        ek.value = key.name
    else:
        entry.string_key = str(key)  # best-effort fallback
    _set_inbound_value(entry.value, value, kwarg_name=kwarg_name, registered=registered)


def encode_call_args(kwargs: Dict[str, Any], call_id: int) -> bytes:
    """Encode function keyword arguments as `CallFunctionArgs` protobuf.

    Release tradeoff: a callable that encodes successfully is registered in
    the per-process host-value table and is normally released only when the
    engine garbage-collects the `HostClosure` it allocated and fires the C
    release callback (a GC-timed release, drained by the engine after
    collection). If a *later* kwarg fails to encode, though, the
    `CallFunctionArgs` is never sent, so the engine never decodes
    — and so never releases — the callables we registered for earlier kwargs.
    To avoid leaking them (and their strong reference to the user's callable)
    for the life of the process, we track every key registered during this
    encode and release them all if any kwarg fails.
    """
    if call_id == 0:
        raise ValueError("call_id must be a nonzero uint64")
    registered: List[int] = []
    try:
        args = baml_inbound_pb2.CallFunctionArgs()
        args.call_id = call_id
        for key, value in kwargs.items():
            _set_inbound_map_entry(
                args.kwargs.add(), key, value, kwarg_name=key, registered=registered
            )
        return args.SerializeToString()
    except BaseException:
        # Roll back any host callables registered before the failure.
        for key in registered:
            try:
                release_host_callable(key)
            except Exception:
                pass  # best-effort cleanup; never mask the original error
        raise


# ---------------------------------------------------------------------------
# Decoding: BamlOutboundValue → Python values (09e §3)
# ---------------------------------------------------------------------------


_MEDIA_KIND_SUBPATHS = {
    baml_outbound_pb2.MediaTypeEnum.IMAGE: "baml.media.Image",
    baml_outbound_pb2.MediaTypeEnum.AUDIO: "baml.media.Audio",
    baml_outbound_pb2.MediaTypeEnum.VIDEO: "baml.media.Video",
    baml_outbound_pb2.MediaTypeEnum.PDF: "baml.media.Pdf",
}


def _baml_ty_to_python_type(baml_ty: baml_outbound_pb2.BamlTy, type_map: BamlTypeMap) -> Any:
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
    if which == "bigint_type":
        # Python's `int` is arbitrary-precision; BAML's `bigint` shares the
        # same surface, distinguished only by the proto type tag for
        # round-trips (mirrors `translate_ty.rs` for codegen-time).
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
        fqn = _MEDIA_KIND_SUBPATHS.get(baml_ty.media_type.media)
        if fqn is None:
            return typing.Any
        try:
            return type_map.get_class(fqn)
        except BamlError:
            return typing.Any
    if which == "class_type":
        cls = type_map.get_class(baml_ty.class_type.name.name)
        return _parameterize(cls, baml_ty.class_type.name.generic_args, type_map)
    if which == "enum_type":
        return type_map.get_enum(baml_ty.enum_type.name)
    if which == "type_alias_type":
        alias = type_map.get_type_alias(baml_ty.type_alias_type.name.name)
        return _parameterize(alias, baml_ty.type_alias_type.name.generic_args, type_map)
    if which == "list_type":
        return List[_baml_ty_to_python_type(baml_ty.list_type.item_type, type_map)]  # type: ignore[valid-type]
    if which == "map_type":
        return Dict[  # type: ignore[valid-type]
            _baml_ty_to_python_type(baml_ty.map_type.key_type, type_map),
            _baml_ty_to_python_type(baml_ty.map_type.value_type, type_map),
        ]
    if which == "optional_type":
        return Optional[_baml_ty_to_python_type(baml_ty.optional_type.value, type_map)]  # type: ignore[valid-type]
    if which == "union_variant_type":
        cls = type_map.get_class(baml_ty.union_variant_type.name.name)
        return _parameterize(cls, baml_ty.union_variant_type.name.generic_args, type_map)
    raise BamlError(f"Unsupported BamlTy variant {which!r} in generic arg")


def _parameterize(cls, generic_args, type_map: BamlTypeMap):
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
    py_args = tuple(_baml_ty_to_python_type(g.ty, type_map) for g in generic_args)
    try:
        if len(py_args) == 1:
            return cls[py_args[0]]
        return cls[py_args]
    except (TypeError, AttributeError):
        return cls


# Single-underscore "private" field names that codegen emits for
# handle-backed stdlib classes. Source of truth: `rg '\$rust_type'
# baml_language/crates/baml_builtins2/`.
_HANDLE_FIELD_NAMES = ("_handle", "_data", "_body")


def _decode_class(class_value, type_map: BamlTypeMap) -> Any:
    """Resolve a `BamlValueClass` to a typed Pydantic model instance.

    Children are decoded first, so `model_validate` receives an
    already-typed field dict — validation mostly acts as a shape check.
    Generic classes parameterize before validation so static-checker
    annotations (`Box[int]`) line up with the runtime instance —
    `13b` §3.4.
    """
    field_dict = {
        entry.key: decode_value(entry.value, type_map)
        for entry in class_value.fields
    }
    # Emit always fully qualifies, so the engine FQN already matches
    # what the typemap consumes (`12a-namespace-rules.md §5`).
    try:
        cls = type_map.get_class(class_value.name.name)
    except BamlError:
        # Bare bridge tests and other non-generated runtimes may receive
        # stdlib/user error classes without a generated typemap installed.
        # Preserve the thrown value's fields instead of masking the original
        # error with an "Unknown class FQN" decode failure.
        return field_dict

    # Media stdlib classes (`baml.media.*`) are PyO3 types wrapping a
    # `BamlPyHandle`. The engine emits them as
    # `class_value { name: "baml.media.Pdf", fields: { _data: handle_value }}`;
    # the inner `_data` decode already constructed a fresh `BamlPdf` via
    # `_decode_handle` → `cls._from_pyhandle(...)`. Unwrap and return it
    # directly — `BamlPdf` is not a Pydantic model.
    if cls in _MEDIA_PYO3_TYPES and "_data" in field_dict:
        return field_dict["_data"]

    parameterized = _parameterize(cls, class_value.name.generic_args, type_map)
    if not _is_pydantic_model_class(cls):
        # Not a BaseModel — shouldn't happen for a well-formed SDK; fall
        # back to a plain dict so callers aren't silently lied to.
        return field_dict

    # Separate handle-backed private attrs from regular fields. Pydantic
    # v2 doesn't accept private attrs via kwargs; we set them on
    # `__pydantic_private__` post-construction.
    private_fields = {
        k: field_dict.pop(k) for k in _HANDLE_FIELD_NAMES if k in field_dict
    }
    instance = parameterized.model_validate(field_dict)
    if private_fields:
        if instance.__pydantic_private__ is None:
            instance.__pydantic_private__ = {}
        for name, value in private_fields.items():
            instance.__pydantic_private__[name] = value
    return instance


def _decode_enum(enum_value, type_map: BamlTypeMap) -> Any:
    """Resolve a `BamlValueEnum` to a member of the generated enum class."""
    variant = enum_value.value
    fqn = enum_value.name.name
    cls = type_map.get_enum(fqn)
    try:
        return cls(variant)
    except ValueError as exc:
        raise BamlError(
            f"BEX returned variant {variant!r} that does not name a "
            f"member of {fqn!r}"
        ) from exc


def _decode_handle(handle, type_map: BamlTypeMap) -> Any:
    """Wrap `HANDLE_TABLE[handle.key]` in a `BamlPyHandle` and dispatch
    on the wire `handle.handle_type` field. The `BamlPyHandle` itself
    holds the `handle_type` tag (set at construction from the wire) so
    it round-trips on inbound encode without needing to consult the
    table; we read directly from the wire here to avoid a redundant
    field access.

    `handle` is either an outbound `BamlOutboundHandle` (carries `name`
    for `ADT_TAGGED_HEAP_HANDLE` dispatch — see 23a) or an inbound
    `BamlHandle` shape (no `name`). The tests pass the inbound shape
    directly; the production path goes through `_decode_value_holder`,
    which hands us the outbound shape.
    """
    HT = baml_inbound_pb2.BamlHandleType
    ht = handle.handle_type
    pyhandle = BamlPyHandle(handle.key, int(ht))

    if ht == HT.ADT_MEDIA_IMAGE:
        return BamlImage._from_pyhandle(pyhandle)
    if ht == HT.ADT_MEDIA_AUDIO:
        return BamlAudio._from_pyhandle(pyhandle)
    if ht == HT.ADT_MEDIA_VIDEO:
        return BamlVideo._from_pyhandle(pyhandle)
    if ht == HT.ADT_MEDIA_PDF:
        return BamlPdf._from_pyhandle(pyhandle)
    if ht == HT.ADT_TAGGED_HEAP_HANDLE:
        # Dispatch via the typemap: every tagged-handle class self-
        # registers under its engine FQN (25b §2), so any future class
        # is reachable without touching this arm.
        name = getattr(handle, "name", None)
        class_fqn = name.name if name is not None else ""
        cls = type_map.get_class(class_fqn)
        return cls._from_pyhandle(pyhandle)
    if ht == HT.HANDLE_UNSPECIFIED:
        raise BamlError("BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)")

    # Everything else (UNTAGGED_RUST_DATA, UNTAGGED_BEX_HEAP, FUNCTION_REF,
    # ADT_PROMPT_AST, ADT_COLLECTOR, ADT_TYPE, ADT_MEDIA_GENERIC): bare
    # BamlPyHandle. The outer codegen class (if any) wraps it via
    # `_decode_class` → private-attr injection.
    return pyhandle


# Workspace bigint cap = 2^28 bits ⇒ at most (2^28)/4 hex digits (plus a
# small slack), matching the Rust-side `MAX_BIGINT_HEX_LEN` in
# `bridge_ctypes/src/value_decode.rs`. Reject longer inputs before calling
# `int(..., 16)` so a malicious wire payload can't drive an unbounded
# Python `int` allocation.
_MAX_BIGINT_HEX_LEN = (1 << 28) // 4 + 2


def _parse_hex_bigint(s: str) -> int:
    # Strip exactly one leading minus (matching the other bridges).
    negative = s.startswith("-")
    magnitude = s[1:] if negative else s
    if len(magnitude) > _MAX_BIGINT_HEX_LEN:
        raise ValueError(
            f"bigint hex exceeds the workspace cap ({len(magnitude)} chars, "
            f"limit {_MAX_BIGINT_HEX_LEN})"
        )
    # Strict hex: reject the `0x` prefixes, underscores, and surrounding
    # whitespace that `int(s, 16)` would otherwise silently accept, matching
    # the encoders' output and the Rust/JS bridges.
    if not magnitude or not all(c in "0123456789abcdefABCDEF" for c in magnitude):
        raise ValueError(f"invalid bigint hex string: {s!r}")
    value = int(magnitude, base=16)
    return -value if negative else value


def _decode_literal(literal) -> Any:
    which = literal.WhichOneof("literal")
    if which == "string_literal":
        return literal.string_literal.value
    if which == "int_literal":
        return literal.int_literal.value
    if which == "bool_literal":
        return literal.bool_literal.value
    if which == "bigint_literal":
        # Hex / base sixteen on the wire, matching `bigint_value`. The
        # helper guards against megabyte-scale payloads before parsing.
        return _parse_hex_bigint(literal.bigint_literal.value)
    return None


def decode_value(holder, type_map: BamlTypeMap) -> Any:
    """Convert a `BamlOutboundValue` message to a typed Python value.

    Every recursive call threads the same `BamlTypeMap` — the only seam
    where the dispatcher consults the process-global registry is the
    outer `decode_call_result` callsite, which passes `get_type_map()`.
    Tests build a fresh `BamlTypeMap` and call this directly.
    """
    which = holder.WhichOneof("value")
    if which is None or which == "null_value":
        return None
    if which == "string_value":
        return holder.string_value
    if which == "int_value":
        return holder.int_value
    if which == "bigint_value":
        # Hex / base sixteen on the wire; the helper guards against
        # megabyte-scale payloads before parsing.
        return _parse_hex_bigint(holder.bigint_value)
    if which == "float_value":
        return holder.float_value
    if which == "bool_value":
        return holder.bool_value
    if which == "uint8array_value":
        return holder.uint8array_value
    if which == "literal_value":
        return _decode_literal(holder.literal_value)
    if which == "list_value":
        return [decode_value(item, type_map) for item in holder.list_value.items]
    if which == "map_value":
        return {
            entry.key: decode_value(entry.value, type_map)
            for entry in holder.map_value.entries
        }
    if which == "class_value":
        return _decode_class(holder.class_value, type_map)
    if which == "enum_value":
        return _decode_enum(holder.enum_value, type_map)
    if which == "union_variant_value":
        # Union metadata is discarded — Python is duck-typed. The inner
        # value self-describes (09e §3).
        return decode_value(holder.union_variant_value.value, type_map)
    if which == "handle_value":
        return _decode_handle(holder.handle_value, type_map)
    if which in ("media_value", "prompt_ast_value"):
        raise BamlError(
            f"BEX emitted {which!r} on the FFI path — media/prompt AST "
            f"are expected via handle_value, not inline"
        )
    return None


def _try_rehydrate_host_value(decoded: Any) -> Optional[BaseException]:
    """If `decoded` is a `baml.errors.HostCallable` pydantic instance
    whose `_handle` points at a still-live entry in this runtime's
    host-value registry, return the *original* Python exception object.
    Otherwise return `None` (foreign runtime, released key, or
    unexpected shape) so the caller falls back to the metadata-bearing
    `BamlError` wrapper.
    """
    private = getattr(decoded, "__pydantic_private__", None)
    if not isinstance(private, dict):
        return None
    handle = private.get("_handle")
    if handle is None:
        return None
    from .baml_py import lookup_host_value

    original = lookup_host_value(handle)
    if isinstance(original, BaseException):
        return original
    return None


def _unwrap_union_variant(holder):
    """Peel any `union_variant_value` wrapper(s) so a metadata read sees the
    inner value. The engine wraps a thrown value in `union_variant_value` when
    the function declares a multi-member `throws` union; `decode_value` already
    unwraps this for the value itself, so the FQN read must match or
    `class_name` is lost for union throws."""
    while holder.WhichOneof("value") == "union_variant_value":
        holder = holder.union_variant_value.value
    return holder


def _outbound_class_fqn(holder) -> Optional[str]:
    """The BAML FQN of a `BamlOutboundValue` that is a class instance (e.g.
    `baml.json.JsonParseError`), else `None`. Used only to build a readable
    `BamlError` / `BamlPanic` message."""
    holder = _unwrap_union_variant(holder)
    if holder.WhichOneof("value") == "class_value":
        return holder.class_value.name.name
    return None


def decode_call_result(data: bytes) -> Any:
    """Decode a `BamlOutboundResult` envelope to a Python value, raising
    `BamlError` / `BamlPanic` for the thrown arms (31c / 31f).

    - `ok` → the decoded return value.
    - `error` → `raise BamlError` with `.value` = decoded value,
      `.baml_trace` = the pre-rendered frame lines.
    - `panic` → if `is_exit_panic`, flush telemetry and `os._exit(exit_code)`
      (a clean `baml.sys.exit` — terminate the whole process from any
      thread/task, *not* a catchable `SystemExit`); otherwise
      `raise BamlPanic` likewise.
    """
    result = baml_outbound_pb2.BamlOutboundResult()
    result.ParseFromString(data)
    which = result.WhichOneof("result")
    type_map = get_type_map()

    if which == "error":
        msg = result.error
        decoded = decode_value(msg.value, type_map)
        # Same-host rehydration: a `baml.errors.HostCallable` carrying a
        # `_handle` that still resolves in this runtime's host-value
        # registry re-raises the *original* native exception object the
        # bridge registered on the inbound throw — preserving `raised is
        # caught` identity. Foreign runtimes (different process) and
        # released keys (last `HostValueArc` clone already dropped) fall
        # through to the metadata-bearing `BamlError` wrapper below.
        if _outbound_class_fqn(msg.value) == "baml.errors.HostCallable":
            rehydrated = _try_rehydrate_host_value(decoded)
            if rehydrated is not None:
                raise attach_baml_traceback(rehydrated)
        raise attach_baml_traceback(
            BamlError(
                decoded,
                baml_trace=list(msg.trace),
                class_name=_outbound_class_fqn(msg.value),
            )
        )

    if which == "panic":
        msg = result.panic
        # Check the discriminator *before* decoding — an exit doesn't need its
        # `baml.panics.Exit` payload decoded to act.
        if msg.is_exit_panic:
            _flush_for_exit()
            os._exit(msg.exit_code)
        raise attach_baml_traceback(
            BamlPanic(
                decode_value(msg.value, type_map),
                baml_trace=list(msg.trace),
                class_name=_outbound_class_fqn(msg.value),
            )
        )

    # `ok` (or an absent oneof — an all-default envelope is a null `ok`).
    return decode_value(result.ok, type_map)


def _flush_for_exit() -> None:
    """Best-effort flush of buffered telemetry before `os._exit`, which
    bypasses `atexit` / buffer flushing. Never raises — exit must proceed."""
    try:
        from .baml_py import flush_events

        flush_events()
    except Exception:
        pass
