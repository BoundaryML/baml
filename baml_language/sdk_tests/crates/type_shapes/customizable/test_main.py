"""Smoke tests for the type_shapes sdk-test crate.

The actual type-shape verification happens in `pyright baml_sdk` (run by
the `pyright` test in `tests/sdk_test.rs`). These pytest cases just
confirm each generated namespace imports cleanly and that the symbols
listed in 18a are reachable.
"""


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_all_namespaces_reachable():
    import baml_sdk.primitives  # noqa: F401
    import baml_sdk.media  # noqa: F401
    import baml_sdk.enums  # noqa: F401
    import baml_sdk.literals  # noqa: F401
    import baml_sdk.class_refs  # noqa: F401
    import baml_sdk.aliases  # noqa: F401
    import baml_sdk.aliases_consumer  # noqa: F401
    import baml_sdk.optional  # noqa: F401
    import baml_sdk.lists  # noqa: F401
    import baml_sdk.maps  # noqa: F401
    import baml_sdk.unions  # noqa: F401
    import baml_sdk.recursion  # noqa: F401
    import baml_sdk.generics  # noqa: F401
    import baml_sdk.forward_refs  # noqa: F401
    import baml_sdk.lorem  # noqa: F401
    import baml_sdk.a  # noqa: F401


def test_root_foo_reachable():
    from baml_sdk import Foo  # noqa: F401


def test_lorem_resume_reachable():
    from baml_sdk.lorem import Resume  # noqa: F401


def test_deep_namespace_thing_reachable():
    from baml_sdk.a.b import Thing  # noqa: F401
