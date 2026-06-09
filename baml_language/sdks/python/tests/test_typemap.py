"""Unit tests for baml_core.typemap (25a2 §4.1).

These tests exercise the FQN → class registry in isolation; no codegen,
no proto, no runtime. Each test uses a fresh BamlTypeMap instance to
avoid bleed-through from process-global state populated by other tests.

25b2: the eager `register_class` / `register_enum` / `register_type_alias`
API is gone. Typemaps are built via `BamlTypeMap.from_lazy_entries(...)`
from codegen-emitted `FQN → (module_path, attr_name)` dicts; resolution
happens on first `get_*` call via `importlib.import_module + getattr`.
"""
from __future__ import annotations

import pytest

from baml_core import BamlError
from baml_core.typemap import BamlTypeMap


def test_from_lazy_entries_resolves_class_via_importlib():
    """Lazy entries point at a (module, attr) pair that gets resolved on
    first `get_class(fqn)` lookup, then cached."""
    tm = BamlTypeMap.from_lazy_entries(
        # Use a stdlib class for the test; any module + attr pair works.
        classes={"std.collections.OrderedDict": ("collections", "OrderedDict")},
        enums={},
        type_aliases={},
    )
    import collections
    cls = tm.get_class("std.collections.OrderedDict")
    assert cls is collections.OrderedDict
    # Cached on second lookup — same object.
    assert tm.get_class("std.collections.OrderedDict") is collections.OrderedDict


def test_get_class_unknown_fqn_raises():
    tm = BamlTypeMap()
    with pytest.raises(BamlError, match="Unknown class FQN"):
        tm.get_class("user.lorem.Nope")


def test_get_enum_unknown_fqn_raises():
    tm = BamlTypeMap()
    with pytest.raises(BamlError, match="Unknown enum FQN"):
        tm.get_enum("user.lorem.Nope")


def test_get_type_alias_unknown_fqn_raises():
    tm = BamlTypeMap()
    with pytest.raises(BamlError, match="Unknown type alias FQN"):
        tm.get_type_alias("user.lorem.Nope")


def test_get_class_unresolvable_module_raises():
    """A lazy entry that points at a missing module bubbles up as a
    descriptive `BamlError` with the failing `module.attr` path."""
    tm = BamlTypeMap.from_lazy_entries(
        classes={"user.lorem.Mystery": ("nonexistent.package", "Mystery")},
        enums={},
        type_aliases={},
    )
    with pytest.raises(BamlError, match="nonexistent.package.Mystery"):
        tm.get_class("user.lorem.Mystery")


def test_py_type_to_baml_type_walks_mro():
    """The reverse map keys on `(__module__, __qualname__)` and walks
    `cls.__mro__` so user subclasses of generated classes resolve to
    the parent's FQN (matching the deleted ClassVar's inheritance)."""
    import collections
    tm = BamlTypeMap.from_lazy_entries(
        classes={"std.collections.OrderedDict": ("collections", "OrderedDict")},
        enums={},
        type_aliases={},
    )

    class MyOrderedDict(collections.OrderedDict):
        pass

    assert tm.py_type_to_baml_type(MyOrderedDict) == "std.collections.OrderedDict"


def test_py_type_to_baml_type_returns_empty_for_unknown():
    """Reverse lookup is informational-only on the wire; unknown
    classes return the empty string, never raise."""
    tm = BamlTypeMap()

    class Unrelated:
        pass

    assert tm.py_type_to_baml_type(Unrelated) == ""


def test_stdlib_reverse_overrides_seeded():
    """Every typemap seeds the PyO3-identity → `baml.media.*` /
    `baml.llm.Stream` reverse-map overrides at construction time."""
    from baml_core.baml_py import BamlImage, BamlAudio, BamlVideo, BamlPdf
    from baml_core import BamlStream

    tm = BamlTypeMap()
    assert tm.py_type_to_baml_type(BamlImage) == "baml.media.Image"
    assert tm.py_type_to_baml_type(BamlAudio) == "baml.media.Audio"
    assert tm.py_type_to_baml_type(BamlVideo) == "baml.media.Video"
    assert tm.py_type_to_baml_type(BamlPdf) == "baml.media.Pdf"
    assert tm.py_type_to_baml_type(BamlStream) == "baml.llm.Stream"
