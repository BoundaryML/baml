"""Protobuf encoder/decoder for the BAML bridge_ctypes protocol.

Inbound: Python builds the control part of `CallFunctionArgs` (identity,
target, explicit type arguments, invocation options). The argument values
themselves are not encoded here: the bridge walks them against the types
the program declares for the target's parameters (`bridge_encode`), so a
value that does not fit raises a `TypeError` before anything is sent.

Outbound (09e): `baml_outbound.proto` → Python value. Decoding is driven
by the FQN metadata embedded on `BamlValueClass` / `BamlValueEnum` and
the wire `BamlHandle.handle_type` tag (read by `_decode_handle`); the
caller's declared Python return type plays no runtime role.
"""

from __future__ import annotations

import asyncio
import enum
import keyword
import os
import types as python_types
import typing
from collections import Counter
from typing import Any, Dict, List, Optional, Tuple

from .cffi.v1 import baml_handle_pb2, baml_inbound_pb2, baml_outbound_pb2, baml_type_pb2
from .baml_py import (
    BamlAudio,
    BamlImage,
    BamlPdf,
    BamlPyHandle,
    BamlVideo,
    cancel_function_call,
    get_runtime as _get_runtime,
    new_function_call,
    release_function_call,
    _release_wire_handle,
)
from ._stream import BamlStream
from ._function_spec import BamlFunctionSpec
from ._runtime_value import BamlOpaque, BamlRuntimeValue
from .errors import BamlCancelledError, BamlError, BamlPanic, attach_baml_traceback
from .typemap import BamlClassInstance, TypeLookup, get_type_map


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
# `_decode_class` unwrap (15b §line 21). The class-shaped `_data` payload
# remains an implementation shell around the native handle.
_MEDIA_PYO3_TYPES = (BamlImage, BamlAudio, BamlVideo, BamlPdf)
# Media Python types as wire `BamlTy` media kinds, for `_types=` lowering.
_MEDIA_WIRE_KINDS = {
    BamlImage: baml_type_pb2.BAML_TY_MEDIA_KIND_IMAGE,
    BamlAudio: baml_type_pb2.BAML_TY_MEDIA_KIND_AUDIO,
    BamlVideo: baml_type_pb2.BAML_TY_MEDIA_KIND_VIDEO,
    BamlPdf: baml_type_pb2.BAML_TY_MEDIA_KIND_PDF,
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


def _live_keys(message: Any) -> typing.Iterator[int]:
    """Every `live` head under a wire type, once per occurrence: each is a row
    of the engine's handle table that names a runtime declaration."""
    for field, value in message.ListFields():
        if (
            field.name == "live"
            and field.containing_oneof is not None
            and field.containing_oneof.name == "head"
        ):
            yield value
        elif field.type == field.TYPE_MESSAGE:
            if field.is_repeated:
                for item in value:
                    yield from _live_keys(item)
            else:
                yield from _live_keys(value)


class _LiveRows:
    """The engine rows a type value's runtime declarations are held by, one
    per occurrence, released together when the last type sharing them goes."""

    __slots__ = ("_handles",)

    def __init__(self, keys: typing.Iterable[int]) -> None:
        untagged = int(baml_handle_pb2.BamlHandleType.UNTAGGED_BEX_HEAP)
        self._handles = [BamlPyHandle(key, untagged) for key in keys]


class BamlType:
    """Opaque host handle for a reflected BAML type.

    The protobuf payload is intentionally private: the supported host surface
    is composition (`meta`, `array`, `optional`) and passing the value back to
    BAML. A runtime declaration the type names is held by the engine row it
    arrived under; a type sent back states it with a reference of the call's
    own. Python object identity is not BAML type identity.
    """

    __slots__ = ("_ty", "_rows")

    def __init__(self, ty: "baml_type_pb2.BamlTy", rows: Optional[_LiveRows] = None) -> None:
        copied = baml_type_pb2.BamlTy()
        copied.CopyFrom(ty)
        self._ty = copied
        self._rows = rows

    @classmethod
    def _adopt(cls, ty: "baml_type_pb2.BamlTy") -> "BamlType":
        """A type the engine handed over: its `live` heads are rows this
        value now holds."""
        return cls(ty, _LiveRows(_live_keys(ty)))

    @classmethod
    def _from_python(cls, value: Any) -> "BamlType":
        if isinstance(value, cls):
            return value
        return cls(python_type_to_wire_ty(value))

    def _wire_copy(self) -> "baml_type_pb2.BamlTy":
        copied = baml_type_pb2.BamlTy()
        copied.CopyFrom(self._ty)
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
        composed = baml_type_pb2.BamlTy()
        composed.list.item.CopyFrom(self._ty)
        return BamlType(composed, self._rows)

    def optional(self) -> "BamlType":
        composed = baml_type_pb2.BamlTy()
        composed.optional.inner.CopyFrom(self._ty)
        return BamlType(composed, self._rows)

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


class _EncodedCall(bytes):
    """The control part of a call as `CallFunctionArgs` bytes, with the
    values the bridge encodes against the target's declared types alongside.

    `kwargs` are the arguments by declared parameter name and `cancel` the
    `_baml.cancel` token, both still Python objects; `BamlRuntime.call_function`
    takes all three. `_retained` keeps borrowed invocation capabilities alive
    through native preparation."""

    kwargs: Dict[str, Any]
    cancel: Any
    _retained: Any

    def __new__(cls, wire, retained, kwargs, cancel):
        value = super().__new__(cls, wire)
        value._retained = retained
        value.kwargs = kwargs
        value.cancel = cancel
        return value


def encode_call_args(
    kwargs: Dict[str, Any],
    call_id: int,
    type_args: Optional[List[Tuple[str, Any]]] = None,
    *,
    function_name: Optional[str] = None,
    function_handle: Optional[int] = None,
    _baml: Any = None,
    _stream_step: bool = False,
) -> _EncodedCall:
    """Build the control part of a `CallFunctionArgs` for a call.

    The argument values are not encoded here. They ride the returned
    `_EncodedCall` as Python objects, and the bridge walks them against the
    types the program declares for the target's parameters; every owner that
    walk creates is released again if a value does not fit.
    """
    if call_id == 0:
        raise ValueError("call_id must be a nonzero uint64")
    try:
        if function_name is not None and function_handle is not None:
            raise ValueError("exactly one BAML call target may be set")
        args = baml_inbound_pb2.CallFunctionArgs()
        args.call_id = call_id
        from ._invocation import normalize

        controls, cancel, retained = normalize(_baml, call_id)
        if _stream_step and controls.trace.WhichOneof("selection") != "reservation":
            # Keep explicit context/capture patches while leaving pulls in
            # timing mode. The retained stream function span records errors.
            if controls.trace.options.mode != baml_inbound_pb2.TRACE_MODE_HIDDEN:
                controls.trace.options.mode = baml_inbound_pb2.TRACE_MODE_TIMING
        args.invocation.CopyFrom(controls)
        if function_name is not None:
            args.function_name = function_name
        elif function_handle is not None:
            args.function_handle = function_handle
        if type_args:
            for type_var, wire_ty in type_args:
                entry = args.type_args.add()
                entry.type_var = type_var
                # The bridge restates a runtime declaration with a reference
                # of the call's own; the wrapper keeps its rows.
                entry.type_value.CopyFrom(
                    wire_ty._ty if isinstance(wire_ty, BamlType) else wire_ty
                )
        return _EncodedCall(args.SerializeToString(), retained, dict(kwargs), cancel)
    except BaseException:
        release_function_call(call_id)
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


def _ty_to_python_type(ty: "baml_type_pb2.BamlTy", type_map: TypeLookup) -> Any:
    """Walk a wire `BamlTy` (`baml_type.proto`) and return the corresponding Python
    type, used for `cls[args]` parameterization on decode.

    The runtime inverse of `python_type_to_wire_ty` / `_fill_wire_ty` in the
    same module (the inbound writer) and the mirror of the engine's
    `ty_encode::runtime_ty_to_proto_ty`. A position with no concrete Python
    binding (a structural union, a type variable, an opaque/runtime-only type,
    a name that `type_map` has no Python type for) widens to `typing.Any` — an
    unbound wildcard for parameterization purposes (02a §5).
    """
    which = ty.WhichOneof("ty")
    # Absent variant or the unknown/top type → wildcard.
    if which is None or which == "unknown":
        return typing.Any
    if which == "primitive":
        return _TY_PRIMITIVE_PY.get(ty.primitive.kind, typing.Any)
    if which == "class_ty":
        cls = type_map.get_class(ty.class_ty.name)
        if cls is None:
            return typing.Any
        return _parameterize_tys(cls, ty.class_ty.type_args, type_map)
    if which == "enum":
        enum_cls = type_map.get_enum(ty.enum.name)
        return typing.Any if enum_cls is None else enum_cls
    if which == "type_alias":
        alias = type_map.get_type_alias(ty.type_alias.name)
        if alias is None:
            return typing.Any
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
            media_cls = type_map.get_class(fqn)
        except BamlError:
            return typing.Any
        return typing.Any if media_cls is None else media_cls
    # enum_variant / interface / function / future / rust_type / meta_type /
    # resource / prompt_ast / type_var /
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


def _parameterize_tys(cls, type_args, type_map: TypeLookup):
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
    if source == "file_content":
        # Content read from a file keeps the file's name.
        content = media.file_content
        return cls.from_file_content(content.name, content.base64, mime_type=mime_type)
    constructor = getattr(cls, f"from_{source}")
    return constructor(getattr(media, source), mime_type=mime_type)


def _decode_prompt_ast(prompt_ast, type_map: TypeLookup) -> Any:
    """Reconstruct the generated ``ai.Prompt`` wrapper around owned data."""
    cls = type_map.get_class("ai.Prompt")
    portable = _PortablePromptAst(prompt_ast)
    if cls is None:
        return BamlClassInstance("ai.Prompt", {"_data": portable})
    if not _is_pydantic_model_class(cls):
        raise BamlError("The generated ai.Prompt host type is not a Pydantic model")

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


def _decode_class(
    class_value, type_map: TypeLookup, ownership: Optional[Counter[int]] = None
) -> Any:
    """Resolve a `BamlValueClass` to a typed Pydantic model instance.

    Children are decoded first, so `model_validate` receives an
    already-typed field dict — validation mostly acts as a shape check.
    Generic classes parameterize before validation so static-checker
    annotations (`Box[int]`) line up with the runtime instance —
    `13b` §3.4.

    A class that `type_map` has no Python class for decodes to a
    `BamlClassInstance`.
    """
    field_dict = {
        entry.key: decode_value(entry.value, type_map, ownership)
        for entry in class_value.fields
    }
    # Emit always fully qualifies, so the engine FQN already matches
    # what the typemap consumes (`12a-namespace-rules.md §5`).
    cls = type_map.get_class(class_value.name)
    if cls is None:
        return BamlClassInstance(class_value.name, field_dict)

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


def _decode_enum(enum_value, type_map: TypeLookup) -> Any:
    """Resolve a `BamlValueEnum` to a member of the generated enum class."""
    variant = enum_value.value
    fqn = enum_value.name
    try:
        cls = type_map.get_enum(fqn)
    except BamlError:
        cls = None
    if cls is None:
        # Runtime-created enums have no generated Python class. The loose host
        # representation is their post-alias variant name (H-5).
        return variant
    try:
        return cls(variant)
    except ValueError as exc:
        raise BamlError(
            f"BEX returned variant {variant!r} that does not name a member of {fqn!r}"
        ) from exc


def _opaque_class(handle, type_map: TypeLookup) -> Optional[type]:
    """The generated class of a runtime value the engine keeps to itself
    (`reflect.Package`, ...), from the compiled class its `ty` names; `None`
    for any other runtime value, which keeps the `BamlRuntimeValue` proxy."""
    ty = getattr(handle, "ty", None)
    if (
        ty is None
        or ty.WhichOneof("ty") != "class_ty"
        or ty.class_ty.WhichOneof("head") != "name"
    ):
        return None
    try:
        cls = type_map.get_class(ty.class_ty.name)
    except BamlError:
        return None
    if isinstance(cls, type) and issubclass(cls, BamlOpaque):
        return cls
    return None


def _decode_handle(
    handle, type_map: TypeLookup, ownership: Optional[Counter[int]] = None
) -> Any:
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
    # Creating the base wrapper transfers one reference, even if constructing
    # a typed wrapper or validating its enclosing model subsequently fails.
    if ownership is not None and ht not in (
        HT.HOST_VALUE_CALLABLE,
        HT.HOST_VALUE_OPAQUE,
    ):
        ownership[handle.key] -= 1

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
        opaque = _opaque_class(handle, type_map)
        if opaque is not None:
            return opaque._from_wire(pyhandle, handle.key)
        return BamlRuntimeValue._from_pyhandle(pyhandle)
    if ht == HT.FUNCTION_REF:
        ty = getattr(handle, "ty", None)
        function_ty = ty.function if ty is not None else baml_type_pb2.BamlTyFunction()
        return BamlClosure(pyhandle, function_ty)
    if ht == HT.HANDLE_UNSPECIFIED:
        raise BamlError("BEX emitted HANDLE_UNSPECIFIED (Rust-side bug)")

    # Everything else (UNTAGGED_RUST_DATA, UNTAGGED_BEX_HEAP, FUNCTION_REF,
    # ADT_PROMPT_AST, ADT_TYPE, ADT_MEDIA_GENERIC): bare
    # BamlPyHandle. The outer codegen class (if any) wraps it via
    # `_decode_class` → private-attr injection.
    return pyhandle


def _returned_param_aliases(names: List[str]) -> Dict[str, str]:
    """Mirror codegen's host-name allocation for callable wire parameters."""
    protected = {"_baml", "_types"}
    used = {"self", *protected}
    aliases = {}
    # Authored legal spellings win over projected or control spellings.
    for raw in sorted(names, key=lambda name: (
        not (name not in protected and name.isidentifier() and not keyword.iskeyword(name)),
        name,
    )):
        name = "".join(
            ch if ch == "_" or ch.isalnum() and (index > 0 or ch.isalpha()) else "_"
            for index, ch in enumerate(raw)
        ) or "_"
        if keyword.iskeyword(name) or raw in protected:
            name += "_"
        while name in used:
            name += "_"
        used.add(name)
        if name != raw:
            aliases[name] = raw
    return aliases


class BamlClosure:
    """A reusable BAML callable with synchronous and asynchronous entry paths."""

    __slots__ = ("_handle", "_required_names", "_optional_names", "_aliases")

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
        self._aliases = _returned_param_aliases(
            self._required_names + self._optional_names
        )

    def _to_pyhandle(self) -> BamlPyHandle:
        return self._handle

    def _encode_args(
        self, args: Tuple[Any, ...], kwargs: Dict[str, Any]
    ) -> Tuple[int, _EncodedCall]:
        options = kwargs.pop("_baml", None)
        if "_types" in kwargs:
            raise TypeError("specialized BAML callable handles do not accept _types")
        if len(args) > len(self._required_names):
            raise TypeError(
                f"got {len(args)} positional arguments but this BAML closure "
                f"accepts {len(self._required_names)}"
            )
        values = dict(zip(self._required_names, args))
        allowed = set(self._required_names) | set(self._optional_names)
        for name, value in kwargs.items():
            from . import UNSET

            if value is UNSET:
                continue
            name = self._aliases.get(name, name)
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
            _baml=options,
        )
        return call_id, args_proto

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        _, args_proto = self._encode_args(args, kwargs)
        result_bytes = _get_runtime().call_function_sync(args_proto, args_proto.kwargs, args_proto.cancel)
        return decode_call_result(result_bytes)

    async def call_async(self, *args: Any, **kwargs: Any) -> Any:
        """Invoke on the current application loop, with its current context."""
        from . import _decode_call_result_async

        call_id, args_proto = self._encode_args(args, kwargs)
        try:
            result_bytes = await _get_runtime().call_function(args_proto, args_proto.kwargs, args_proto.cancel)
        except asyncio.CancelledError:
            cancel_function_call(call_id)
            raise
        return _decode_call_result_async(result_bytes)

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


def _host_call_handle_keys(holder) -> typing.Iterator[int]:
    """Yield each owned wire reference; borrowed host registry keys are excluded."""
    which = holder.WhichOneof("value")
    if which == "handle_value":
        handle = holder.handle_value
        if handle.handle_type not in (
            baml_handle_pb2.HOST_VALUE_CALLABLE,
            baml_handle_pb2.HOST_VALUE_OPAQUE,
        ):
            yield handle.key
    elif which == "list_value":
        for item in holder.list_value.items:
            yield from _host_call_handle_keys(item)
    elif which == "map_value":
        for entry in holder.map_value.entries:
            yield from _host_call_handle_keys(entry.value)
    elif which == "class_value":
        for field in holder.class_value.fields:
            yield from _host_call_handle_keys(field.value)
    elif which == "union_variant_value":
        yield from _host_call_handle_keys(holder.union_variant_value.value)
    elif which == "ty_value":
        yield from _live_keys(holder.ty_value)


def _decode_host_call_args(data: bytes) -> Tuple[Tuple[Any, ...], Dict[str, Any]]:
    """Own every wire reference until a host wrapper takes that reference."""
    call = baml_outbound_pb2.BamlToHostCall()
    call.ParseFromString(data)
    ownership = Counter(
        key for arg in call.args for key in _host_call_handle_keys(arg.value)
    )
    try:
        type_map = get_type_map()
        positional = []
        optional = {}
        for arg in call.args:
            value = decode_value(arg.value, type_map, ownership)
            if arg.is_optional_arg:
                optional[arg.arg_name] = value
            else:
                positional.append(value)
        return tuple(positional), optional
    finally:
        for key, count in ownership.items():
            for _ in range(count):
                try:
                    _release_wire_handle(key)
                except RuntimeError:
                    # Cleanup must preserve the original decode failure.
                    pass


def decode_value(
    holder, type_map: TypeLookup, ownership: Optional[Counter[int]] = None
) -> Any:
    """Convert a `BamlOutboundValue` message to a typed Python value.

    Every recursive call threads the same `type_map` — the only seam
    where the dispatcher consults the process-global registry is the
    outer `decode_call_result` callsite, which passes `get_type_map()`.
    Tests build a fresh `BamlTypeMap` and call this directly.

    `type_map` also decides what a class with no Python class is. A
    `BamlTypeMap` raises for it: the caller of a typed function must not get
    a stand-in for the model it declares. `BamlTypeMap.where_generated`
    decodes its instances to `BamlClassInstance`, for a value that has no
    declared Python type.
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
        return [
            decode_value(item, type_map, ownership) for item in holder.list_value.items
        ]
    if which == "map_value":
        return {
            entry.key: decode_value(entry.value, type_map, ownership)
            for entry in holder.map_value.entries
        }
    if which == "class_value":
        return _decode_class(holder.class_value, type_map, ownership)
    if which == "enum_value":
        return _decode_enum(holder.enum_value, type_map)
    if which == "union_variant_value":
        # Union metadata is discarded — Python is duck-typed. The inner
        # value self-describes (09e §3).
        return decode_value(holder.union_variant_value.value, type_map, ownership)
    if which == "handle_value":
        return _decode_handle(holder.handle_value, type_map, ownership)
    if which == "ty_value":
        if ownership is not None:
            for key in _live_keys(holder.ty_value):
                ownership[key] -= 1
        return BamlType._adopt(holder.ty_value)
    if which == "media_value":
        return _decode_media(holder.media_value)
    if which == "prompt_ast_value":
        return _decode_prompt_ast(holder.prompt_ast_value, type_map)
    return None


def _try_rehydrate_host_value(decoded: Any) -> Optional[BaseException]:
    """If `decoded` is a `baml.errors.HostCallable` instance (of the
    generated class, or a `BamlClassInstance` where there is none)
    whose `_handle` points at a still-live entry in this runtime's
    host-value registry, return the *original* Python exception object.
    Otherwise return `None` (foreign runtime, released key, or
    unexpected shape) so the caller falls back to the metadata-bearing
    `BamlError` wrapper.
    """
    if isinstance(decoded, BamlClassInstance):
        handle = decoded.fields.get("_handle")
    else:
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
    `baml.json.ParseError`), else `None`. Used only to build a readable
    `BamlError` / `BamlPanic` message."""
    holder = _unwrap_union_variant(holder)
    if holder.WhichOneof("value") == "class_value":
        return holder.class_value.name
    return None


def decode_call_result(data: bytes) -> Any:
    """Decode a `BamlOutboundResult` envelope to a Python value, raising
    `BamlError` / `BamlPanic` for the thrown arms (31c / 31f).

    - `ok` → the decoded return value. A class with no Python class is an
      error: the return value has a declared type that a typed caller relies
      on.
    - `error` → `raise BamlError` with `.value` = decoded value,
      `.baml_trace` = the pre-rendered frame lines. A thrown value has no
      declared Python type and must reach the caller, so an instance of a
      class with no Python class is a `BamlClassInstance` here and in a
      panic: an error of the decode would replace the error that was thrown.
    - `panic` → if `is_exit_panic`, `os._exit(exit_code)`
      (a clean `baml.sys.exit` — terminate the whole process from any
      thread/task, *not* a catchable `SystemExit`); otherwise
      `raise BamlPanic` likewise.
    """
    result = baml_outbound_pb2.BamlOutboundResult()
    result.ParseFromString(data)
    which = result.WhichOneof("result")
    type_map = get_type_map()
    thrown_types = type_map.where_generated()

    if which == "error":
        msg = result.error
        decoded = decode_value(msg.value, thrown_types)
        # A value/type mismatch at the call boundary (`baml.errors.TypeMismatch`,
        # synthesized host-side from `EngineError::TypeMismatch`) is a *caller*
        # type error — surface it as Python's native `TypeError` rather than a
        # `BamlError` wrapper. Covers inbound-generics Gate-A failures (a
        # `TypeVar` that can't be inferred and must be specified, conflicting
        # variance occurrences) and ordinary argument-type mismatches.
        if _outbound_class_fqn(msg.value) == "baml.errors.TypeMismatch":
            if isinstance(decoded, BamlClassInstance):
                message = decoded.fields.get("message")
            else:
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
            os._exit(msg.exit_code)
        panic_type = (
            BamlCancelledError
            if _outbound_class_fqn(msg.value) == "baml.panics.Cancelled"
            else BamlPanic
        )
        raise attach_baml_traceback(
            panic_type(
                decode_value(msg.value, thrown_types),
                baml_trace=list(msg.trace),
                class_name=_outbound_class_fqn(msg.value),
            )
        )

    # `ok` (or an absent oneof — an all-default envelope is a null `ok`).
    return decode_value(result.ok, type_map)
