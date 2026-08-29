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
import types as python_types
import typing
from typing import Any, Dict, List, Optional, Tuple

from .cffi.v1 import baml_handle_pb2, baml_inbound_pb2, baml_outbound_pb2, baml_type_pb2
from .baml_py import (
    BamlAudio,
    BamlImage,
    BamlPdf,
    BamlPyHandle,
    BamlVideo,
    get_runtime as _get_runtime,
    new_function_call,
    register_host_callable,
    _release_wire_handle,
    release_host_callable,
)
from ._stream import BamlStream
from ._function_spec import BamlFunctionSpec
from ._runtime_value import BamlRuntimeValue
from .errors import BamlCancelledError, BamlError, BamlPanic, attach_baml_traceback
from .typemap import BamlTypeMap, get_type_map


def _is_pydantic_model(value: Any) -> bool:
    try:
        from pydantic import BaseModel  # type: ignore[import-untyped]
    except ImportError:
        return False
    return isinstance(value, BaseModel)


def _is_pydantic_model_class(cls: type) -> bool:
    try:
        from pydantic import BaseModel  # type: ignore[import-untyped]
    except ImportError:
        return False
    return issubclass(cls, BaseModel)


# Media PyO3 types — kept as a tuple for `isinstance` dispatch in the
# encoder and `_decode_class` unwrap (15b §line 21). Their sparse inbound
# annotation is the exact primitive media kind; the class-shaped `_data`
# payload remains an implementation shell around the native handle.
_MEDIA_PYO3_TYPES = (BamlImage, BamlAudio, BamlVideo, BamlPdf)
_MEDIA_WIRE_KINDS = {
    BamlImage: baml_type_pb2.BAML_TY_MEDIA_KIND_IMAGE,
    BamlAudio: baml_type_pb2.BAML_TY_MEDIA_KIND_AUDIO,
    BamlVideo: baml_type_pb2.BAML_TY_MEDIA_KIND_VIDEO,
    BamlPdf: baml_type_pb2.BAML_TY_MEDIA_KIND_PDF,
}
_MEDIA_PROTO_KINDS = {
    BamlImage: baml_outbound_pb2.IMAGE,
    BamlAudio: baml_outbound_pb2.AUDIO,
    BamlVideo: baml_outbound_pb2.VIDEO,
    BamlPdf: baml_outbound_pb2.PDF,
}
_PROTO_MEDIA_TYPES = {
    baml_outbound_pb2.IMAGE: BamlImage,
    baml_outbound_pb2.AUDIO: BamlAudio,
    baml_outbound_pb2.VIDEO: BamlVideo,
    baml_outbound_pb2.PDF: BamlPdf,
}


class _PortablePromptAst:
    """Owned wire copy stored in the generated ``ai.Prompt._data`` slot."""

    __slots__ = ("_value",)

    def __init__(self, value: baml_outbound_pb2.BamlValuePromptAst) -> None:
        self._value = baml_outbound_pb2.BamlValuePromptAst()
        self._value.CopyFrom(value)

    def wire_copy(self) -> baml_outbound_pb2.BamlValuePromptAst:
        copied = baml_outbound_pb2.BamlValuePromptAst()
        copied.CopyFrom(self._value)
        return copied


class BamlType:
    """Opaque host handle for a reflected BAML type definition.

    The protobuf payload is intentionally private: the supported host surface
    is composition (`meta`, `array`, `optional`) and passing the value back to
    BAML. Python object identity is not BAML type identity.
    """

    __slots__ = ("_definition",)

    def __init__(self, definition: "baml_type_pb2.BamlTyDef") -> None:
        copied = baml_type_pb2.BamlTyDef()
        copied.CopyFrom(definition)
        self._definition = copied

    @classmethod
    def _from_python(cls, value: Any) -> "BamlType":
        if isinstance(value, cls):
            return value
        definition = baml_type_pb2.BamlTyDef()
        definition.root.CopyFrom(python_type_to_wire_ty(value))
        return cls(definition)

    def _wire_copy(self) -> "baml_type_pb2.BamlTyDef":
        copied = baml_type_pb2.BamlTyDef()
        copied.CopyFrom(self._definition)
        return copied

    def meta(
        self,
        *,
        alias: Optional[str] = None,
        description: Optional[str] = None,
        docstring: Optional[str] = None,
        other: Optional[Dict[str, str]] = None,
    ) -> "_BamlTypeMetadataRow":
        return _BamlTypeMetadataRow(
            self,
            alias=alias,
            description=description,
            docstring=docstring,
            other=dict(other or {}),
        )

    def array(self) -> "BamlType":
        definition = self._wire_copy()
        old_root = baml_type_pb2.BamlTy()
        old_root.CopyFrom(definition.root)
        definition.root.Clear()
        definition.root.list.item.CopyFrom(old_root)
        return BamlType(definition)

    def optional(self) -> "BamlType":
        definition = self._wire_copy()
        old_root = baml_type_pb2.BamlTy()
        old_root.CopyFrom(definition.root)
        definition.root.Clear()
        definition.root.optional.inner.CopyFrom(old_root)
        return BamlType(definition)

    def __reduce__(self):
        raise TypeError("BamlType values are runtime handles and cannot be serialized")

    def __reduce_ex__(self, protocol: int):
        raise TypeError("BamlType values are runtime handles and cannot be serialized")

    def __repr__(self) -> str:
        return "BamlType(<opaque>)"


class _BamlTypeMetadataRow:
    __slots__ = ("ty", "alias", "description", "docstring", "other")

    def __init__(
        self,
        type: BamlType,
        *,
        alias: Optional[str],
        description: Optional[str],
        docstring: Optional[str],
        other: Dict[str, str],
    ) -> None:
        self.ty = type
        self.alias = alias
        self.description = description
        self.docstring = docstring
        self.other = other


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
        subpath = f"{module[len(prefix) :]}.{name}"

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
        inner = _subpath_to_baml_fqn(subpath[len("stream_types.") :])
        return f"{inner}$stream" if inner else ""
    if subpath.startswith("vendor."):
        return subpath[len("vendor.") :]
    if subpath.startswith("baml."):
        return subpath
    return f"user.{subpath}"


def _safe_sdk_root() -> str:
    """Fetch `sdk_root` without raising if the runtime is uninitialized.

    FQN derivation is diagnostic, so it must not promote a missing runtime
    into a hard failure on the inbound path.
    """
    try:
        from . import (
            get_runtime,
        )  # local import: avoids circular binding at module load

        return get_runtime()._sdk_root or ""
    except Exception:
        return ""


def _set_inbound_value(
    inbound_value: baml_inbound_pb2.InboundValue,
    value: Any,
    *,
    kwarg_name: str,
    registered: Optional[List[int]] = None,
    cloned_handles: Optional[List[int]] = None,
) -> None:
    """Populate an `InboundValue` oneof from a Python value per 09d §2.

    `kwarg_name` is threaded through so the `TypeError` raised on
    unsupported inputs names the offending top-level kwarg, not the
    nested field we happen to have descended into.

    `registered` and `cloned_handles`, when supplied, collect the two kinds of
    ownership created during encoding: host-value registry keys for callables,
    and HANDLE_TABLE keys cloned for wire transfer. If encoding aborts before
    the bytes reach the engine, callers explicitly release both sets. On
    success the engine owns them and performs the normal release/drain.
    """
    if value is None:
        return  # oneof unset ≡ null

    if isinstance(value, BamlType):
        inbound_value.ty_def_value.CopyFrom(value._definition)
        return

    if isinstance(value, _BamlTypeMetadataRow):
        cv = inbound_value.class_value
        for key in ("ty", "alias", "description", "docstring", "other"):
            _set_inbound_map_entry(
                cv.fields.add(),
                key,
                getattr(value, key),
                kwarg_name=kwarg_name,
                registered=registered,
                cloned_handles=cloned_handles,
            )
        return

    # `enum.Enum` must precede the primitive arms. Codegen emits enums as
    # mixin subclasses of their backing primitive — `SomeEnum(str, enum.Enum)`
    # (and likewise `int`-backed enums) — so a bare `isinstance(value, str)` /
    # `isinstance(value, int)` check would otherwise swallow an enum member and
    # encode it as `string_value` / `int_value`, losing the `T = SomeEnum`
    # binding the engine recovers from an `EnumVariant`. Same precedence logic
    # as `bool` before `int`: dispatch on the most specific runtime shape first.
    if isinstance(value, enum.Enum):
        ev = inbound_value.enum_value
        ev.name = get_type_map().py_type_to_baml_type(_base_class_for_fqn(type(value)))
        if not isinstance(value.value, str):
            raise TypeError(
                f"Cannot encode enum member {value!r}: BAML enum wire values must be strings"
            )
        ev.value = value.value
        return

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
                list_val.values.add(),
                item,
                kwarg_name=kwarg_name,
                registered=registered,
                cloned_handles=cloned_handles,
            )
        return
    if isinstance(value, dict):
        map_val = inbound_value.map_value
        # Same as the empty-list case above: set the oneof arm so an empty
        # dict encodes as an empty map rather than an unset (null) value.
        map_val.SetInParent()
        for k, v in value.items():
            _set_inbound_map_entry(
                map_val.entries.add(),
                k,
                v,
                kwarg_name=kwarg_name,
                registered=registered,
                cloned_handles=cloned_handles,
            )
        return

    # `BamlPyHandle` is its own top-level inbound variant — peer to
    # `class_value` / `enum_value` / etc. Must precede `pydantic.BaseModel`
    # so a future `BamlPyHandle` subclass would land here, and must
    # precede the media-class branch since the media types compose a
    # `BamlPyHandle` internally and recurse here on `_to_pyhandle()`.
    if isinstance(value, BamlPyHandle):
        key, ht = value._clone_key_for_wire()
        if cloned_handles is not None:
            try:
                cloned_handles.append(key)
            except BaseException:
                try:
                    _release_wire_handle(key)
                except Exception:
                    pass
                raise
        inbound_value.handle.key = key
        # Wire field stays populated for cross-bridge compat. The proto
        # field is typed as the enum class, but `BamlHandleType` is an
        # `int` subclass so the runtime accepts a bare int — cast for
        # the static checker.
        inbound_value.handle.handle_type = typing.cast(
            baml_handle_pb2.BamlHandleType, ht
        )
        return

    if isinstance(value, _PortablePromptAst):
        inbound_value.prompt_ast_value.CopyFrom(value.wire_copy())
        return

    # `BamlStream` (21b §"Phase 4"): lifted to a bare `handle_value` on
    # the wire — the engine intercepts the outer Stream class at
    # `convert_heap_ptr_to_external_with_type` and reconstructs the heap
    # pointer in `convert_external_to_vm_value`'s `Adt(TaggedHeapHandle)`
    # arm. So encode as `handle_value(ADT_TAGGED_HEAP_HANDLE)` rather
    # than the media-style `class_value(name, _data: handle_value)` wrap.
    # Inbound stays a bare `BamlHandle` (key + type only) since the
    # engine's `HANDLE_TABLE` row already carries the receiver's `ty`.
    if isinstance(value, (BamlStream, BamlFunctionSpec, BamlRuntimeValue)):
        return _set_inbound_value(
            inbound_value,
            value._to_pyhandle(),
            kwarg_name=kwarg_name,
            registered=registered,
            cloned_handles=cloned_handles,
        )

    # Media is data, not an engine capability. Copy its canonical payload
    # directly onto the wire; the engine reconstructs the stdlib wrapper in
    # the destination context.
    if isinstance(value, _MEDIA_PYO3_TYPES):
        media = inbound_value.media_value
        media.media = _MEDIA_PROTO_KINDS[type(value)]
        mime_type = value.mime_type()
        if mime_type is not None:
            media.mime_type = mime_type
        if (url := value.url()) is not None:
            media.url = url
        elif (base64 := value.base64()) is not None:
            media.base64 = base64
        elif (file := value.file()) is not None:
            media.file = file
        else:
            raise TypeError(f"Cannot encode empty media argument {kwarg_name!r}")
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
    if (
        callable(value)
        and not isinstance(value, type)
        and not _is_pydantic_model(value)
    ):
        key = register_host_callable(value)
        # Record the key so the encode path can release it if a later
        # kwarg fails to encode (the call never reaches the engine, so the
        # engine would never decode — and never release — this key).
        if registered is not None:
            registered.append(key)
        inbound_value.handle.key = key
        inbound_value.handle.handle_type = typing.cast(
            baml_handle_pb2.BamlHandleType,
            baml_handle_pb2.BamlHandleType.HOST_VALUE_CALLABLE,
        )
        return

    if _is_pydantic_model(value):
        private = getattr(value, "__pydantic_private__", None) or {}
        prompt_data = private.get("_data")
        if isinstance(prompt_data, _PortablePromptAst):
            inbound_value.prompt_ast_value.CopyFrom(prompt_data.wire_copy())
            return

        # Generated field names are Python-safe projections. In particular,
        # ``ai.Prompt._data`` is exposed as ``field_data`` with wire alias
        # ``_data``. Detect the portable payload by that alias so a Prompt is
        # flattened back to its owned AST rather than encoded as a class shell.
        model_fields = type(value).model_fields
        field_values = dict(value)
        for name, candidate in field_values.items():
            field = model_fields.get(name)
            wire_name = (
                field.serialization_alias or field.alias or name if field else name
            )
            if wire_name == "_data" and isinstance(candidate, _PortablePromptAst):
                inbound_value.prompt_ast_value.CopyFrom(candidate.wire_copy())
                return

        cv = inbound_value.class_value
        # Bind the class via sparse node-level `value_type`. A parameterized
        # Pydantic generic (`Box[int]`) carries its exact concrete args. An
        # unparameterized generic carries only nominal class identity (no
        # args); the engine can refine that hint from one contextual class but
        # will not use it to choose between multiple concrete instantiations.
        instance_type_args = pydantic_instance_type_args(value)
        inbound_value.value_type.class_ty.name = get_type_map().py_type_to_baml_type(
            _base_class_for_fqn(type(value))
        )
        for arg in instance_type_args:
            _fill_inner(inbound_value.value_type.class_ty.type_args.add(), arg)
        # Walk fields by attribute access (Pydantic v2's `__iter__`
        # yields `(name, value)` without recursive serialization).
        # `model_dump()` would flatten nested Pydantic instances into
        # dicts and lose the type info — the Rust-side coercer would
        # then see them as `Map` instead of `Instance`, so a
        # `Box<Box<int>>` round-trip collapses into bare dicts at the
        # second level.
        for k, v in field_values.items():
            field = model_fields.get(k)
            wire_name = (field.serialization_alias or field.alias or k) if field else k
            _set_inbound_map_entry(
                cv.fields.add(),
                wire_name,
                v,
                kwarg_name=kwarg_name,
                registered=registered,
                cloned_handles=cloned_handles,
            )
        # Private attrs aren't iterated by `dict(value)`. Codegen emits
        # `$rust_type` fields as private attrs (single-underscore names);
        # walk them explicitly so `BamlPyHandle`-backed shells round-trip.
        # `__pydantic_private__` is None when the model declares no
        # private attrs.
        for k, v in private.items():
            if isinstance(v, (BamlPyHandle, _PortablePromptAst)):
                _set_inbound_map_entry(
                    cv.fields.add(),
                    k,
                    v,
                    kwarg_name=kwarg_name,
                    registered=registered,
                    cloned_handles=cloned_handles,
                )
        return

    raise TypeError(
        f"Cannot encode argument {kwarg_name!r} of type "
        f"{type(value).__name__} into baml_inbound.proto"
    )


def _set_inbound_map_entry(
    entry,
    key: Any,
    value: Any,
    *,
    kwarg_name: str,
    registered: Optional[List[int]] = None,
    cloned_handles: Optional[List[int]] = None,
) -> None:
    """Populate an `InboundMapEntry` from a (key, value) pair. Key-oneof
    dispatch follows 09d §2 "Map keys"; `bool` precedes `int` (subclass).

    The ownership trackers are threaded to `_set_inbound_value` for
    encode-error rollback (see that function)."""
    if isinstance(key, bool):
        entry.bool_key = key
    elif isinstance(key, enum.Enum):
        # Precede `str`/`int`: codegen enums mix in their backing primitive
        # (`SomeEnum(str, enum.Enum)`), so an enum member must be matched here
        # before the `str`/`int` arms would swallow it as a plain scalar key.
        ek = entry.enum_key
        ek.name = get_type_map().py_type_to_baml_type(_base_class_for_fqn(type(key)))
        if not isinstance(key.value, str):
            raise TypeError(
                f"Cannot encode enum key {key!r}: BAML enum wire values must be strings"
            )
        ek.value = key.value
    elif isinstance(key, str):
        entry.string_key = key
    elif isinstance(key, int):
        entry.int_key = key
    else:
        entry.string_key = str(key)  # best-effort fallback
    _set_inbound_value(
        entry.value,
        value,
        kwarg_name=kwarg_name,
        registered=registered,
        cloned_handles=cloned_handles,
    )


# ---------------------------------------------------------------------------
# Type-as-value encoding: Python type → wire `BamlTy` (baml_type.proto)
# ---------------------------------------------------------------------------
#
# Used to bind a generic function/method's TypeVars at a host call: the host
# supplies an explicit type (`_types=`) and/or a generic receiver carries its
# class type args. Each Python type is lowered to a wire `BamlTy`, sent in
# `CallFunctionArgs.type_args`, and seeded into the engine's entry frame.


def python_type_to_wire_ty(py_type: Any) -> "baml_type_pb2.BamlTy":
    """Lower an accepted Python type token to a wire `BamlTy`.

    Unsupported classes are rejected at the call site (H-10); silently
    widening them to `unknown` would make `_types=` appear to succeed while
    discarding the caller's binding.
    """
    ty = baml_type_pb2.BamlTy()
    _fill_wire_ty(ty, py_type)
    return ty


_PRIMITIVE_KINDS = {
    bool: baml_type_pb2.BAML_TY_PRIMITIVE_BOOL,
    int: baml_type_pb2.BAML_TY_PRIMITIVE_INT,
    float: baml_type_pb2.BAML_TY_PRIMITIVE_FLOAT,
    str: baml_type_pb2.BAML_TY_PRIMITIVE_STRING,
    bytes: baml_type_pb2.BAML_TY_PRIMITIVE_BYTES,
    type(None): baml_type_pb2.BAML_TY_PRIMITIVE_NULL,
}


def _collect_never_types() -> tuple:
    """`Never` (BAML's bottom type) may be imported from `typing` (3.11+) or
    `typing_extensions`; codegen emits the `typing_extensions` form. Collect
    every available spelling so a host's `_types={"T": Never}` matches whichever
    it imported."""
    found = []
    for mod_name in ("typing", "typing_extensions"):
        try:
            mod = __import__(mod_name)
        except ImportError:
            continue
        never = getattr(mod, "Never", None)
        if never is not None:
            found.append(never)
    return tuple(found)


_NEVER_TYPES = _collect_never_types()


def _fill_wire_ty(ty: "baml_type_pb2.BamlTy", py_type: Any) -> None:
    # `None` (the sentinel, not `type(None)`) means "unbound" → unknown/top.
    if py_type is None:
        ty.unknown.SetInParent()
        return

    if py_type is typing.Any:
        ty.unknown.SetInParent()
        return

    if isinstance(py_type, BamlType):
        raise TypeError(
            "a BamlType definition handle must be passed directly, not nested in typing"
        )

    # `Never` (bottom type). Identity check (not `in`) so unhashable special
    # forms can't raise.
    if any(py_type is never for never in _NEVER_TYPES):
        ty.never.SetInParent()
        return

    # Primitives by identity. `bool` must precede `int` (bool ⊂ int), which the
    # dict ordering and `is` identity handle since we key on the type object.
    kind = _PRIMITIVE_KINDS.get(py_type)
    if kind is not None:
        ty.primitive.kind = kind
        return

    if py_type in _MEDIA_PYO3_TYPES:
        ty.media.kind = _MEDIA_WIRE_KINDS[py_type]
        return

    # typing constructs: list[X], dict[K, V], Optional[X], Union[...].
    origin = typing.get_origin(py_type)
    if origin is not None:
        targs = typing.get_args(py_type)
        interface_fqn = getattr(origin, "__baml_interface_fqn__", None)
        if interface_fqn:
            ty.interface.name = interface_fqn
            for arg in targs:
                _fill_inner(ty.interface.type_args.add(), arg)
            return
        if origin in (list, typing.List):
            _fill_inner(ty.list.item, targs[0] if targs else None)
            return
        if origin in (dict, typing.Dict):
            _fill_inner(ty.map.key, targs[0] if targs else None)
            _fill_inner(ty.map.value, targs[1] if len(targs) > 1 else None)
            return
        if origin in (typing.Union, python_types.UnionType):
            non_none = [a for a in targs if a is not type(None)]
            if len(non_none) == 1 and len(non_none) != len(targs):
                _fill_inner(ty.optional.inner, non_none[0])
                return
            for arg in targs:
                _fill_inner(ty.union.options.add(), arg)
            return
        if origin is typing.Literal:
            if len(targs) != 1:
                raise TypeError(
                    "BAML type tokens require Literal with exactly one value"
                )
            literal = targs[0]
            if isinstance(literal, bool):
                ty.literal.bool_value = literal
            elif isinstance(literal, int):
                if -(1 << 63) <= literal < (1 << 63):
                    ty.literal.int_value = literal
                else:
                    ty.literal.bigint_value = str(literal)
            elif isinstance(literal, float):
                ty.literal.float_value = repr(literal)
            elif isinstance(literal, str):
                ty.literal.string_value = literal
            else:
                raise TypeError(f"unsupported BAML Literal token {literal!r}")
            return
        raise TypeError(f"unsupported Python typing token for BAML: {py_type!r}")

    if isinstance(py_type, type):
        interface_fqn = getattr(py_type, "__baml_interface_fqn__", None)
        if interface_fqn:
            ty.interface.name = interface_fqn
            return
        # Parameterized Pydantic generic (`Box[int]`): the base FQN plus the
        # concrete args recovered from Pydantic's generic metadata.
        meta = getattr(py_type, "__pydantic_generic_metadata__", None)
        if meta and meta.get("origin") is not None:
            base = meta["origin"]
            ty.class_ty.name = get_type_map().py_type_to_baml_type(base)
            for arg in meta.get("args") or ():
                _fill_inner(ty.class_ty.type_args.add(), arg)
            return
        if issubclass(py_type, enum.Enum):
            fqn = get_type_map().py_type_to_baml_type(py_type)
            if fqn:
                ty.enum.name = fqn
                return
        fqn = get_type_map().py_type_to_baml_type(py_type)
        if fqn:
            ty.class_ty.name = fqn
            return

    raise TypeError(
        f"unsupported Python type token for BAML: {py_type!r}; expected a BAML "
        "generated class/enum (or subclass), a builtin/media type, or a supported typing composition"
    )


def _fill_inner(ty: "baml_type_pb2.BamlTy", py_type: Any) -> None:
    _fill_wire_ty(ty, py_type)


def pydantic_instance_type_args(value: Any) -> List[Any]:
    """Concrete class type args of a Pydantic generic *instance* (`Box[int](...)`),
    in declaration order. Empty for non-generic or unparameterized instances."""
    meta = getattr(type(value), "__pydantic_generic_metadata__", None)
    if meta and meta.get("args"):
        return list(meta["args"])
    return []


def encode_call_args(
    kwargs: Dict[str, Any],
    call_id: int,
    type_args: Optional[List[Tuple[str, Any]]] = None,
    *,
    function_name: Optional[str] = None,
    function_handle: Optional[int] = None,
) -> bytes:
    """Encode function keyword arguments as `CallFunctionArgs` protobuf.

    Encoding can create two kinds of owned key: host-callable registry entries
    and HANDLE_TABLE clones of Python capability handles. A successful encode
    transfers both to the engine. If a later value fails, the bytes are never
    sent, so this function explicitly releases every key created so far.
    """
    if call_id == 0:
        raise ValueError("call_id must be a nonzero uint64")
    if function_name is not None and function_handle is not None:
        raise ValueError("exactly one BAML call target may be set")
    registered: List[int] = []
    cloned_handles: List[int] = []
    try:
        args = baml_inbound_pb2.CallFunctionArgs()
        args.call_id = call_id
        if function_name is not None:
            args.function_name = function_name
        elif function_handle is not None:
            args.function_handle = function_handle
        for key, value in kwargs.items():
            _set_inbound_map_entry(
                args.kwargs.add(),
                key,
                value,
                kwarg_name=key,
                registered=registered,
                cloned_handles=cloned_handles,
            )
        if type_args:
            for type_var, wire_ty in type_args:
                entry = args.type_args.add()
                entry.type_var = type_var
                if isinstance(wire_ty, BamlType):
                    entry.type_definition.CopyFrom(wire_ty._definition)
                else:
                    entry.type_value.CopyFrom(wire_ty)
        return args.SerializeToString()
    except BaseException:
        # Roll back any host callables registered before the failure.
        for key in registered:
            try:
                release_host_callable(key)
            except Exception:
                pass  # best-effort cleanup; never mask the original error
        for key in cloned_handles:
            try:
                _release_wire_handle(key)
            except Exception:
                pass  # best-effort cleanup; never mask the original error
        raise


# ---------------------------------------------------------------------------
# Decoding: BamlOutboundValue → Python values (09e §3)
# ---------------------------------------------------------------------------


_TY_PRIMITIVE_PY = {
    baml_type_pb2.BAML_TY_PRIMITIVE_STRING: str,
    baml_type_pb2.BAML_TY_PRIMITIVE_INT: int,
    baml_type_pb2.BAML_TY_PRIMITIVE_FLOAT: float,
    baml_type_pb2.BAML_TY_PRIMITIVE_BOOL: bool,
    baml_type_pb2.BAML_TY_PRIMITIVE_NULL: type(None),
    baml_type_pb2.BAML_TY_PRIMITIVE_BYTES: bytes,
    # Python's `int` is arbitrary-precision; BAML's `bigint` shares the same
    # surface, distinguished only by the proto type tag (mirrors `translate_ty`).
    baml_type_pb2.BAML_TY_PRIMITIVE_BIGINT: int,
}

_TY_MEDIA_FQN = {
    baml_type_pb2.BAML_TY_MEDIA_KIND_IMAGE: "baml.media.Image",
    baml_type_pb2.BAML_TY_MEDIA_KIND_AUDIO: "baml.media.Audio",
    baml_type_pb2.BAML_TY_MEDIA_KIND_VIDEO: "baml.media.Video",
    baml_type_pb2.BAML_TY_MEDIA_KIND_PDF: "baml.media.Pdf",
}


def _ty_to_python_type(ty: "baml_type_pb2.BamlTy", type_map: BamlTypeMap) -> Any:
    """Walk a wire `BamlTy` (`baml_type.proto`) and return the corresponding Python
    type, used for `cls[args]` parameterization on decode.

    The runtime inverse of `python_type_to_wire_ty` / `_fill_wire_ty` in the
    same module (the inbound writer) and the mirror of the engine's
    `ty_encode::runtime_ty_to_proto_ty`. A position with no concrete Python
    binding (a structural union, a type variable, an opaque/runtime-only type)
    widens to `typing.Any` — an unbound wildcard for parameterization purposes
    (02a §5).
    """
    which = ty.WhichOneof("ty")
    # Absent variant or the unknown/top type → wildcard.
    if which is None or which == "unknown":
        return typing.Any
    if which == "primitive":
        return _TY_PRIMITIVE_PY.get(ty.primitive.kind, typing.Any)
    if which == "class_ty":
        cls = type_map.get_class(ty.class_ty.name)
        return _parameterize_tys(cls, ty.class_ty.type_args, type_map)
    if which == "enum":
        return type_map.get_enum(ty.enum.name)
    if which == "type_alias":
        alias = type_map.get_type_alias(ty.type_alias.name)
        return _parameterize_tys(alias, ty.type_alias.type_args, type_map)
    if which == "list":
        return List[_ty_to_python_type(ty.list.item, type_map)]  # type: ignore[valid-type]
    if which == "map":
        return Dict[  # type: ignore[valid-type]
            _ty_to_python_type(ty.map.key, type_map),
            _ty_to_python_type(ty.map.value, type_map),
        ]
    if which == "optional":
        return Optional[_ty_to_python_type(ty.optional.inner, type_map)]  # type: ignore[valid-type]
    if which == "union":
        # Preserve structural unions through the boundary so a generic arg like
        # `Box[int | str]` round-trips as `typing.Union[int, str]` rather than
        # collapsing to a wildcard. Members decode positionally; `typing.Union`
        # flattens/dedups, a single surviving member unwraps to itself, and a
        # null member naturally yields `Optional[...]`. A member that can't bind
        # decodes to `typing.Any` and rides along as a `typing.Any` arm.
        members = tuple(_ty_to_python_type(opt, type_map) for opt in ty.union.options)
        if not members:
            return typing.Any
        return typing.Union[members]  # type: ignore[valid-type]
    if which == "literal":
        inner = _decode_ty_literal(ty.literal)
        if inner is None:
            return typing.Any
        return typing.Literal[inner]  # type: ignore[valid-type]
    if which == "media":
        fqn = _TY_MEDIA_FQN.get(ty.media.kind)
        if fqn is None:
            return typing.Any
        try:
            return type_map.get_class(fqn)
        except BamlError:
            return typing.Any
    # enum_variant / interface / function / future / rust_type / meta_type /
    # resource / prompt_ast / void / type_var /
    # associated_type_projection / never — no concrete Python binding in a
    # generic-arg position; treat as an unbound wildcard.
    return typing.Any


def _decode_ty_literal(literal) -> Any:
    """Decode a wire `BamlTyLiteral` to its Python literal value (for
    `typing.Literal[...]`). Distinct from `_decode_literal`, which decodes the
    value-level `BamlTyLiteral` riding on `BamlOutboundValue.literal_value`."""
    which = literal.WhichOneof("literal")
    if which == "string_value":
        return literal.string_value
    if which == "int_value":
        return literal.int_value
    if which == "bool_value":
        return literal.bool_value
    if which == "bigint_value":
        # Decimal string on the wire (the `BamlTy` convention; see baml_type.proto).
        return int(literal.bigint_value)
    if which == "float_value":
        return float(literal.float_value)
    return None


def _parameterize_tys(cls, type_args, type_map: BamlTypeMap):
    """Apply a list of wire `BamlTy` generic args to a symbol via `cls[arg_types…]`.

    Works for any subscriptable symbol — Pydantic generics, generic type
    aliases (PEP 695 `TypeAliasType` or `typing` aliases like `List[T]`),
    `typing.Generic` subclasses, etc. The try/except catches the failure modes
    (arity mismatch, non-generic class, fully-bound alias) and falls back to the
    unparameterized `cls`.
    """
    type_args = list(type_args)
    if not type_args:
        return cls
    py_args = tuple(_ty_to_python_type(t, type_map) for t in type_args)
    try:
        if len(py_args) == 1:
            return cls[py_args[0]]
        return cls[py_args]
    except (TypeError, AttributeError):
        return cls


def _decode_media(media) -> Any:
    cls = _PROTO_MEDIA_TYPES.get(media.media)
    if cls is None:
        raise BamlError(f"BEX emitted unsupported portable media kind {media.media}")
    source = media.WhichOneof("value")
    if source is None:
        raise BamlError("BEX emitted a portable media value with no content")
    mime_type = media.mime_type if media.HasField("mime_type") else None
    constructor = getattr(cls, f"from_{source}")
    return constructor(getattr(media, source), mime_type=mime_type)


def _decode_prompt_ast(prompt_ast, type_map: BamlTypeMap) -> Any:
    """Reconstruct the generated ``ai.Prompt`` wrapper around owned data."""
    cls = type_map.get_class("ai.Prompt")
    if not _is_pydantic_model_class(cls):
        raise BamlError("The generated ai.Prompt host type is not a Pydantic model")

    portable = _PortablePromptAst(prompt_ast)
    if any(
        (field.serialization_alias or field.alias or name) == "_data"
        for name, field in cls.model_fields.items()
    ):
        # The generated annotation is the VM-side ``$rust_type`` proxy, while
        # this host-owned value deliberately carries a portable AST instead.
        # Construction must therefore bypass Pydantic validation.
        return cls.model_construct(_data=portable)

    # Compatibility with generated Prompt models that represented ``_data``
    # as a Pydantic private attribute.
    instance = cls.model_validate({})
    if instance.__pydantic_private__ is None:
        instance.__pydantic_private__ = {}
    instance.__pydantic_private__["_data"] = portable
    return instance


def _decode_class(class_value, type_map: BamlTypeMap) -> Any:
    """Resolve a `BamlValueClass` to a typed Pydantic model instance.

    Children are decoded first, so `model_validate` receives an
    already-typed field dict — validation mostly acts as a shape check.
    Generic classes parameterize before validation so static-checker
    annotations (`Box[int]`) line up with the runtime instance —
    `13b` §3.4.
    """
    field_dict = {
        entry.key: decode_value(entry.value, type_map) for entry in class_value.fields
    }
    # Emit always fully qualifies, so the engine FQN already matches
    # what the typemap consumes (`12a-namespace-rules.md §5`).
    cls = type_map.get_class(class_value.name)

    # Media stdlib classes (`baml.media.*`) are PyO3 types wrapping a
    # `BamlPyHandle`. The engine emits them as
    # `class_value { name: "baml.media.Pdf", fields: { _data: handle_value }}`;
    # the inner `_data` decode already constructed a fresh `BamlPdf` via
    # `_decode_handle` → `cls._from_pyhandle(...)`. Unwrap and return it
    # directly — `BamlPdf` is not a Pydantic model.
    if cls in _MEDIA_PYO3_TYPES and "_data" in field_dict:
        return field_dict["_data"]

    parameterized = _parameterize_tys(cls, class_value.type_args, type_map)
    if not _is_pydantic_model_class(cls):
        # Not a BaseModel — shouldn't happen for a well-formed SDK; fall
        # back to a plain dict so callers aren't silently lied to.
        return field_dict

    # Separate legacy handle-backed private attrs from regular fields.
    # PythonNames projects generated fields such as `_handle` to a public
    # Python name with `_handle` as its wire alias. Those belong in
    # `model_validate`; only a wire-private name with no generated alias is a
    # Pydantic private attribute that must be installed post-construction.
    model_wire_aliases = {
        alias
        for field in parameterized.model_fields.values()
        for alias in (
            field.validation_alias,
            field.alias,
            field.serialization_alias,
        )
        if isinstance(alias, str)
    }
    private_fields = {
        key: field_dict.pop(key)
        for key, value in list(field_dict.items())
        if key.startswith("_")
        and key not in model_wire_aliases
        and isinstance(value, BamlPyHandle)
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
    fqn = enum_value.name
    try:
        cls = type_map.get_enum(fqn)
    except BamlError:
        # Runtime-created enums have no generated Python class. The loose host
        # representation is their post-alias variant name (H-5).
        return variant
    try:
        return cls(variant)
    except ValueError as exc:
        raise BamlError(
            f"BEX returned variant {variant!r} that does not name a member of {fqn!r}"
        ) from exc


def _decode_handle(handle, type_map: BamlTypeMap) -> Any:
    """Wrap `HANDLE_TABLE[handle.key]` in a `BamlPyHandle` and dispatch
    on the wire `handle.handle_type` field. The `BamlPyHandle` itself
    holds the `handle_type` tag (set at construction from the wire) so
    it round-trips on inbound encode without needing to consult the
    table; we read directly from the wire here to avoid a redundant
    field access.

    `handle` is either an outbound `BamlOutboundHandle` (carries `ty`
    for `ADT_TAGGED_HEAP_HANDLE` dispatch — see 23a) or an inbound
    `BamlHandle` shape (no `ty`). The tests pass the inbound shape
    directly for non-tagged kinds; the production path goes through
    `_decode_value_holder`, which hands us the outbound shape.
    """
    HT = baml_handle_pb2.BamlHandleType
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
        return BamlStream._from_pyhandle(pyhandle)
    if ht == HT.ADT_FUNCTION_SPEC:
        return BamlFunctionSpec._from_pyhandle(pyhandle)
    if ht == HT.ADT_RUNTIME_VALUE:
        return BamlRuntimeValue._from_pyhandle(pyhandle)
    if ht == HT.FUNCTION_REF:
        ty = getattr(handle, "ty", None)
        function_ty = ty.function if ty is not None else baml_type_pb2.BamlTyFunction()
        return BamlClosure(pyhandle, function_ty)
    if ht == HT.HANDLE_UNSPECIFIED:
        raise BamlError("BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)")

    # Everything else (UNTAGGED_RUST_DATA, UNTAGGED_BEX_HEAP, FUNCTION_REF,
    # ADT_PROMPT_AST, ADT_COLLECTOR, ADT_TYPE, ADT_MEDIA_GENERIC): bare
    # BamlPyHandle. The outer codegen class (if any) wraps it via
    # `_decode_class` → private-attr injection.
    return pyhandle


class BamlClosure:
    """A reusable, engine-owned BAML callable."""

    __slots__ = ("_handle", "_required_names", "_optional_names")

    def __init__(self, handle: BamlPyHandle, function_ty: Any):
        mode = baml_type_pb2.BamlTyFunctionParamMode
        self._handle = handle
        self._required_names = [
            param.name if param.HasField("name") else f"arg{index}"
            for index, param in enumerate(function_ty.params)
            if param.mode != mode.BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL
        ]
        self._optional_names = [
            param.name if param.HasField("name") else f"arg{index}"
            for index, param in enumerate(function_ty.params)
            if param.mode == mode.BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL
        ]

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        if len(args) > len(self._required_names):
            raise TypeError(
                f"got {len(args)} positional arguments but this BAML closure "
                f"accepts {len(self._required_names)}"
            )
        values = dict(zip(self._required_names, args))
        allowed = set(self._required_names) | set(self._optional_names)
        for name, value in kwargs.items():
            if name not in allowed:
                raise TypeError(f"unexpected keyword argument {name!r}")
            if name in values:
                raise TypeError(f"multiple values for argument {name!r}")
            values[name] = value
        call_id = new_function_call()
        args_proto = encode_call_args(
            values,
            call_id,
            function_handle=self._handle._key_for_call(),
        )
        result_bytes = _get_runtime().call_function_sync(args_proto)
        return decode_call_result(result_bytes)

    def __repr__(self) -> str:
        return "<BamlClosure>"


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
    if which == "string_value":
        return literal.string_value
    if which == "int_value":
        return literal.int_value
    if which == "bool_value":
        return literal.bool_value
    if which == "bigint_value":
        # Hex / base sixteen on the wire, matching `bigint_value`. The
        # helper guards against megabyte-scale payloads before parsing.
        return _parse_hex_bigint(literal.bigint_value)
    if which == "float_value":
        # Source text on the wire (mirrors `BamlTyLiteral.float_value`).
        return float(literal.float_value)
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
    if which == "ty_value":
        definition = baml_type_pb2.BamlTyDef()
        definition.root.CopyFrom(holder.ty_value)
        return BamlType(definition)
    if which == "ty_def_value":
        return BamlType(holder.ty_def_value)
    if which == "media_value":
        return _decode_media(holder.media_value)
    if which == "prompt_ast_value":
        return _decode_prompt_ast(holder.prompt_ast_value, type_map)
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
    handle = private.get("_handle") if isinstance(private, dict) else None
    if handle is None and _is_pydantic_model_class(type(decoded)):
        values = vars(decoded)
        for name, field in type(decoded).model_fields.items():
            aliases = (
                field.validation_alias,
                field.alias,
                field.serialization_alias,
            )
            if "_handle" in aliases:
                handle = values.get(name)
                break
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
        return holder.class_value.name
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
        # A value/type mismatch at the call boundary (`baml.errors.TypeMismatch`,
        # synthesized host-side from `EngineError::TypeMismatch`) is a *caller*
        # type error — surface it as Python's native `TypeError` rather than a
        # `BamlError` wrapper. Covers inbound-generics Gate-A failures (a
        # `TypeVar` that can't be inferred and must be specified, conflicting
        # variance occurrences) and ordinary argument-type mismatches.
        if _outbound_class_fqn(msg.value) == "baml.errors.TypeMismatch":
            message = getattr(decoded, "message", None)
            if message is None and isinstance(decoded, dict):
                message = decoded.get("message")
            err = TypeError(message if message is not None else str(decoded))
            # Let `attach_baml_traceback` splice the BAML frames onto the
            # native exception (exception instances accept ad-hoc attributes).
            err.baml_trace = list(msg.trace)  # type: ignore[attr-defined]
            raise attach_baml_traceback(err)
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
        panic_type = (
            BamlCancelledError
            if _outbound_class_fqn(msg.value) == "baml.panics.Cancelled"
            else BamlPanic
        )
        raise attach_baml_traceback(
            panic_type(
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
