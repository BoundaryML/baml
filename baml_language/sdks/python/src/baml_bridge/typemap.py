"""Process-global FQN → Python class registry (25a2 §4.1).

The typemap is codegen-emitted: each SDK ships a `baml_sdk/_typemap.py`
that constructs a `BamlTypeMap` from literal dicts of
`FQN → (module_path, attr_name)` lazy entries, and the SDK's root
`__init__.py` installs it via `set_type_map(_TYPE_MAP)`. Resolution
happens on first `get_class(fqn)` call via `importlib.import_module +
getattr`, then memoizes. Interface reference classes have a separate map from
concrete data classes; resolving a Python class never establishes conformance.
Concrete facade lookup additionally requires native static-declaration evidence
matching the SDK's bytecode bundle; diagnostic wire names never select it.
"""

from __future__ import annotations
import importlib
from contextlib import contextmanager
from contextvars import ContextVar
from typing import Dict, Tuple, Type, TYPE_CHECKING

if TYPE_CHECKING:
    from ._interface import BamlInterfaceRef
    from ._concrete import BamlConcreteRef
    from .baml_py import BamlPyHandle

from .errors import BamlError

_LazyEntry = Tuple[str, str]  # (module_path, attr_name)

# Hardcoded reverse-map seeds for the five stdlib re-exports whose
# Python class identity sits at baml_bridge.baml_py.BamlImage etc.
# instead of at baml_sdk.baml.media.Image. The forward emit path
# (sdkgen_python_pydantic2's `media_reexport_rust_name`) keeps its own
# hardcoded match arms; the duplication is small enough that a
# shared source of truth isn't worth a cross-crate constant.
_STDLIB_REVERSE_OVERRIDES: Dict[Tuple[str, str], str] = {
    ("baml_bridge.baml_py", "BamlImage"): "baml.media.Image",
    ("baml_bridge.baml_py", "BamlAudio"): "baml.media.Audio",
    ("baml_bridge.baml_py", "BamlVideo"): "baml.media.Video",
    ("baml_bridge.baml_py", "BamlPdf"): "baml.media.Pdf",
    # `BamlStream` is re-exported from `baml_bridge` but defined in
    # `baml_bridge._stream`; `__module__` reflects the defining module.
    ("baml_bridge._stream", "BamlStream"): "ai.stream.Stream",
    ("baml_bridge._function_spec", "BamlFunctionSpec"): "ai.FunctionSpec",
}


class BamlTypeMap:
    __slots__ = (
        # Lazy entries — codegen-emitted, resolved on first lookup.
        "_class_lazy",
        "_enum_lazy",
        "_alias_lazy",
        "_interface_lazy",
        "_concrete_lazy",
        "_sdk_bundle_id",
        # Resolved cache — populated by first successful lazy resolution.
        "_class_cache",
        "_enum_cache",
        "_alias_cache",
        "_interface_cache",
        "_concrete_cache",
        # Reverse map: (module, qualname) → engine FQN. Populated from
        # forward entries in `from_lazy_entries`, seeded with stdlib
        # PyO3-identity overrides. `py_type_to_baml_type` walks
        # `cls.__mro__` against this dict.
        "_reverse",
    )

    def __init__(self) -> None:
        self._class_lazy: Dict[str, _LazyEntry] = {}
        self._enum_lazy: Dict[str, _LazyEntry] = {}
        self._alias_lazy: Dict[str, _LazyEntry] = {}
        self._interface_lazy: Dict[str, _LazyEntry] = {}
        self._concrete_lazy: Dict[str, _LazyEntry] = {}
        self._sdk_bundle_id: bytes | None = None
        self._class_cache: Dict[str, Type] = {}
        self._enum_cache: Dict[str, Type] = {}
        self._alias_cache: Dict[str, object] = {}
        self._interface_cache: Dict[str, Type[BamlInterfaceRef]] = {}
        self._concrete_cache: Dict[str, Type[BamlConcreteRef]] = {}
        # Seed with the stdlib identity overrides every typemap needs
        # (BamlImage at baml_bridge.baml_py → "baml.media.Image", etc.).
        # Forward entries added by `from_lazy_entries` populate more
        # keys on top.
        self._reverse: Dict[Tuple[str, str], str] = dict(_STDLIB_REVERSE_OVERRIDES)

    @classmethod
    def from_lazy_entries(
        cls,
        classes: Dict[str, _LazyEntry],
        enums: Dict[str, _LazyEntry],
        type_aliases: Dict[str, _LazyEntry],
        interface_refs: Dict[str, _LazyEntry] | None = None,
        concrete_refs: Dict[str, _LazyEntry] | None = None,
        sdk_bundle_id: bytes | None = None,
    ) -> "BamlTypeMap":
        m = cls()
        m._class_lazy = dict(classes)
        m._enum_lazy = dict(enums)
        m._alias_lazy = dict(type_aliases)
        m._interface_lazy = dict(interface_refs or {})
        m._concrete_lazy = dict(concrete_refs or {})
        if m._concrete_lazy and sdk_bundle_id is None:
            raise ValueError(
                "generated concrete references require an SDK bundle identity"
            )
        if sdk_bundle_id is not None and (
            not isinstance(sdk_bundle_id, bytes) or len(sdk_bundle_id) != 32
        ):
            raise ValueError("invalid SDK bundle identity")
        m._sdk_bundle_id = sdk_bundle_id
        # Derive (module, attr) → FQN from forward entries.
        # `setdefault` lets stdlib seeds (populated in __init__) win
        # on collision — for stdlib classes both the user-facing
        # re-export key AND the PyO3 identity key end up in the
        # reverse map; lookups on either resolve to the same FQN.
        for fqn, (mp, attr) in classes.items():
            m._reverse.setdefault((mp, attr), fqn)
        for fqn, (mp, attr) in enums.items():
            m._reverse.setdefault((mp, attr), fqn)
        for fqn, (mp, attr) in m._concrete_lazy.items():
            m._reverse.setdefault((mp, attr), fqn)
        # Type aliases generally don't appear as `type(value)`; skip.
        return m

    # — lookup (lazy fallback) —

    def get_concrete_ref(self, handle: BamlPyHandle) -> Type[BamlConcreteRef]:
        from ._concrete import BamlConcreteRef

        if self._sdk_bundle_id is None:
            return BamlConcreteRef
        # The native table supplies this name after matching the loaded bundle.
        # No wire display name can select a generated class. This inspection
        # works while the aggregate is provisional and grants no call access.
        fqn = handle._sdk_concrete_name(self._sdk_bundle_id)
        if fqn is None:
            return BamlConcreteRef
        return self._load_concrete_ref(fqn)

    def _load_concrete_ref(self, fqn: str) -> Type[BamlConcreteRef]:
        from ._concrete import BamlConcreteRef

        cached = self._concrete_cache.get(fqn)
        if cached is not None:
            return cached
        entry = self._concrete_lazy.get(fqn)
        if entry is None:
            return BamlConcreteRef
        module_path, attr = entry
        try:
            cls = getattr(importlib.import_module(module_path), attr)
        except (ImportError, AttributeError) as exc:
            raise BamlError(f"Could not resolve concrete ref {fqn!r}: {exc}") from exc
        if not isinstance(cls, type) or not issubclass(cls, BamlConcreteRef):
            raise BamlError(f"Concrete ref entry {fqn!r} is not an SDK reference class")
        self._concrete_cache[fqn] = cls
        return cls

    def get_interface_ref(self, fqn: str) -> Type[BamlInterfaceRef]:
        from ._interface import BamlInterfaceRef

        cached = self._interface_cache.get(fqn)
        if cached is not None:
            return cached
        entry = self._interface_lazy.get(fqn)
        if entry is None:
            # Runtime-created declarations may have no generated subclass.
            # Preserve their capability rather than invent a nominal model.
            return BamlInterfaceRef
        module_path, attr = entry
        try:
            cls = getattr(importlib.import_module(module_path), attr)
        except (ImportError, AttributeError) as exc:
            raise BamlError(f"Could not resolve interface ref {fqn!r}: {exc}") from exc
        if not isinstance(cls, type) or not issubclass(cls, BamlInterfaceRef):
            raise BamlError(
                f"Interface ref entry {fqn!r} is not an SDK reference class"
            )
        self._interface_cache[fqn] = cls
        return cls

    def get_class_type(self, fqn: str) -> Type:
        """Resolve a native annotation, without constructing a live receiver.

        Value decoding must still use get_concrete_ref's bundle/declaration
        check. A type name alone cannot select a receiver facade.
        """
        if fqn in self._concrete_lazy:
            return self._load_concrete_ref(fqn)
        return self.get_class(fqn)

    def get_class(self, fqn: str) -> Type:
        cached = self._class_cache.get(fqn)
        if cached is not None:
            return cached
        entry = self._class_lazy.get(fqn)
        if entry is None:
            raise BamlError(
                f"Unknown class FQN {fqn!r}; codegen did not emit a "
                "typemap entry (or codegen drift left it stale)"
            )
        module_path, attr = entry
        try:
            module = importlib.import_module(module_path)
            cls = getattr(module, attr)
        except (ImportError, AttributeError) as exc:
            raise BamlError(
                f"Could not resolve {fqn!r} → {module_path}.{attr}: {exc}"
            ) from exc
        self._class_cache[fqn] = cls
        return cls

    def get_enum(self, fqn: str) -> Type:
        cached = self._enum_cache.get(fqn)
        if cached is not None:
            return cached
        entry = self._enum_lazy.get(fqn)
        if entry is None:
            raise BamlError(f"Unknown enum FQN {fqn!r}")
        module_path, attr = entry
        try:
            module = importlib.import_module(module_path)
            cls = getattr(module, attr)
        except (ImportError, AttributeError) as exc:
            raise BamlError(
                f"Could not resolve enum {fqn!r} → {module_path}.{attr}: {exc}"
            ) from exc
        self._enum_cache[fqn] = cls
        return cls

    def get_type_alias(self, fqn: str) -> object:
        cached = self._alias_cache.get(fqn)
        if cached is not None:
            return cached
        entry = self._alias_lazy.get(fqn)
        if entry is None:
            raise BamlError(f"Unknown type alias FQN {fqn!r}")
        module_path, attr = entry
        try:
            module = importlib.import_module(module_path)
            alias = getattr(module, attr)
        except (ImportError, AttributeError) as exc:
            raise BamlError(
                f"Could not resolve alias {fqn!r} → {module_path}.{attr}: {exc}"
            ) from exc
        self._alias_cache[fqn] = alias
        return alias

    # — reverse lookup (replaces _baml_type_name ClassVar pathway) —

    def py_type_to_baml_type(self, cls: type) -> str:
        """Reverse lookup: Python class → engine FQN. Walks the MRO so
        user subclasses of generated classes resolve to the parent's
        FQN (matching today's ClassVar inheritance). Returns `""` for
        any class not in the typemap — informational-only field on the
        wire, same as 25b's `_derive_baml_fqn` fallback."""
        for c in cls.__mro__:
            fqn = self._reverse.get((c.__module__, c.__qualname__))
            if fqn is not None:
                return fqn
        return ""

    def warm(self) -> None:
        for fqn in list(self._class_lazy):
            self.get_class(fqn)
        for fqn in list(self._enum_lazy):
            self.get_enum(fqn)
        for fqn in list(self._alias_lazy):
            self.get_type_alias(fqn)
        for fqn in list(self._interface_lazy):
            self.get_interface_ref(fqn)


_TYPE_MAP = BamlTypeMap()
_LOCAL_TYPE_MAP: ContextVar[BamlTypeMap | None] = ContextVar(
    "baml_type_map", default=None
)


@contextmanager
def _using_type_map(type_map: BamlTypeMap):
    token = _LOCAL_TYPE_MAP.set(type_map)
    try:
        yield
    finally:
        _LOCAL_TYPE_MAP.reset(token)


def set_type_map(m: BamlTypeMap) -> None:
    global _TYPE_MAP
    _TYPE_MAP = m


def get_type_map() -> BamlTypeMap:
    selected = _LOCAL_TYPE_MAP.get()
    return _TYPE_MAP if selected is None else selected
