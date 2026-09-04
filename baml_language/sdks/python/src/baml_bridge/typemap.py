"""SDK-local type maps, scoped across calls and captured by runtime capabilities."""

from __future__ import annotations
import importlib
import contextvars
import functools
import inspect
import threading
from contextlib import contextmanager
from typing import Dict, Optional, Tuple, Type

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
        "runtime",
        # Lazy entries — codegen-emitted, resolved on first lookup.
        "_class_lazy",
        "_enum_lazy",
        "_alias_lazy",
        # Resolved cache — populated by first successful lazy resolution.
        "_class_cache",
        "_enum_cache",
        "_alias_cache",
        # Reverse map: (module, qualname) → engine FQN. Populated from
        # forward entries in `from_lazy_entries`, seeded with stdlib
        # PyO3-identity overrides. `py_type_to_baml_type` walks
        # `cls.__mro__` against this dict.
        "_reverse",
    )

    def __init__(self) -> None:
        self.runtime = None
        self._class_lazy: Dict[str, _LazyEntry] = {}
        self._enum_lazy: Dict[str, _LazyEntry] = {}
        self._alias_lazy: Dict[str, _LazyEntry] = {}
        self._class_cache: Dict[str, Type] = {}
        self._enum_cache: Dict[str, Type] = {}
        self._alias_cache: Dict[str, object] = {}
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
        sdk_module: str | None = None,
    ) -> "BamlTypeMap":
        if sdk_module:
            def rebase(entries):
                return {fqn: (sdk_module + module[len("baml_sdk"):], attr) for fqn, (module, attr) in entries.items()}
            classes, enums, type_aliases = rebase(classes), rebase(enums), rebase(type_aliases)
        m = cls()
        m._class_lazy = dict(classes)
        m._enum_lazy = dict(enums)
        m._alias_lazy = dict(type_aliases)
        # Derive (module, attr) → FQN from forward entries.
        # `setdefault` lets stdlib seeds (populated in __init__) win
        # on collision — for stdlib classes both the user-facing
        # re-export key AND the PyO3 identity key end up in the
        # reverse map; lookups on either resolve to the same FQN.
        for fqn, (mp, attr) in classes.items():
            m._reverse.setdefault((mp, attr), fqn)
        for fqn, (mp, attr) in enums.items():
            m._reverse.setdefault((mp, attr), fqn)
        # Type aliases generally don't appear as `type(value)`; skip.
        return m

    # — lookup (lazy fallback) —

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


_TYPE_MAP = BamlTypeMap()
_SDK_MAP_LOCK = threading.RLock()
_ACTIVE_MAP: contextvars.ContextVar[Optional[BamlTypeMap]] = contextvars.ContextVar("baml_type_map", default=None)
_SDK_MAPS: Dict[str, BamlTypeMap] = {}
_PROGRAM_MAPS: Dict[int, BamlTypeMap] = {}


def set_type_map(m: BamlTypeMap, runtime=None, sdk_module: str | None = None) -> None:
    global _TYPE_MAP
    m.runtime = runtime
    if sdk_module is None:
        _TYPE_MAP = m
    else:
        with _SDK_MAP_LOCK:
            _SDK_MAPS[sdk_module] = m
            if runtime is not None:
                _PROGRAM_MAPS.setdefault(runtime.runtime_key, m)


def get_type_map() -> BamlTypeMap:
    active = _ACTIVE_MAP.get()
    if active is not None:
        return active
    with _SDK_MAP_LOCK:
        if len(_SDK_MAPS) == 1:
            return next(iter(_SDK_MAPS.values()))
        return _TYPE_MAP


def type_map_for_module(module: str | None) -> BamlTypeMap:
    with _SDK_MAP_LOCK:
        matches = [name for name in _SDK_MAPS if module and (module == name or module.startswith(name + "."))]
        return _SDK_MAPS[max(matches, key=len)] if matches else get_type_map()


@contextmanager
def type_map_scope(type_map: BamlTypeMap):
    token = _ACTIVE_MAP.set(type_map)
    try:
        yield
    finally:
        _ACTIVE_MAP.reset(token)


def bind_type_map(call, resolve):
    """Resolve lazily for generated imports; preserve scope across async suspension."""
    if inspect.iscoroutinefunction(call):
        @functools.wraps(call)
        async def async_call(*args, **kwargs):
            with type_map_scope(resolve()):
                return await call(*args, **kwargs)
        return async_call
    @functools.wraps(call)
    def sync_call(*args, **kwargs):
        with type_map_scope(resolve()):
            return call(*args, **kwargs)
    return sync_call


def runtime_bound(call):
    if inspect.iscoroutinefunction(call):
        @functools.wraps(call)
        async def async_call(self, *args, **kwargs):
            with type_map_scope(self._type_map):
                return await call(self, *args, **kwargs)
        return async_call
    @functools.wraps(call)
    def sync_call(self, *args, **kwargs):
        with type_map_scope(self._type_map):
            return call(self, *args, **kwargs)
    return sync_call


def runtime_argument_bound(call):
    """Bind raw runtime helpers before encoding callbacks or decoding capabilities."""
    def resolve(runtime):
        import copy
        base = get_type_map()
        if base.runtime is not None and base.runtime.runtime_key == runtime.runtime_key:
            return base
        with _SDK_MAP_LOCK:
            registered = _PROGRAM_MAPS.get(runtime.runtime_key)
        if registered is not None:
            return registered
        bound = copy.copy(base)
        bound.runtime = runtime
        return bound
    if inspect.iscoroutinefunction(call):
        @functools.wraps(call)
        async def async_call(runtime, *args, **kwargs):
            with type_map_scope(resolve(runtime)):
                return await call(runtime, *args, **kwargs)
        return async_call
    @functools.wraps(call)
    def sync_call(runtime, *args, **kwargs):
        with type_map_scope(resolve(runtime)):
            return call(runtime, *args, **kwargs)
    return sync_call
