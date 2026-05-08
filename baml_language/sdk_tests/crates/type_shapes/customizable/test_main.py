"""Smoke tests for the type_shapes sdk-test crate.

The actual type-shape verification happens in `pyright baml_sdk` (run by
the `pyright` test in `tests/sdk_test.rs`). These pytest cases just
confirm each generated namespace imports cleanly and that the symbols
listed in 18a are reachable.
"""


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_all_namespaces_reachable():
    import baml_sdk

    # Every directory under baml_src that contains a .baml file becomes a
    # baml_sdk submodule (with `ns_` prefix stripped).
    # Several namespaces from 18a are disabled because the TIR resolver
    # doesn't currently support cross-namespace user-package refs — see
    # 18b for the deferred coverage list.
    # `void` is intentionally absent: its baml_src directory only contains a
    # `function NoOp() -> void {}` and pure-expression functions don't emit
    # Python code (see 18b). Likewise `ns_lorem/streams.baml` only adds
    # functions, so its routing-rule coverage shows up as the `Box` /
    # `Resume` declarations rather than per-function .pyi entries.
    for ns in (
        "primitives",
        "media",
        "enums",
        "literals",
        "class_refs",
        "aliases",
        "aliases_consumer",
        "optional",
        "lists",
        "maps",
        "unions",
        "recursion",
        "generics",
        "forward_refs",
        "lorem",
        "a",
    ):
        assert hasattr(baml_sdk, ns), f"baml_sdk.{ns} missing"


def test_root_foo_reachable():
    from baml_sdk import Foo  # noqa: F401


def test_lorem_resume_reachable():
    from baml_sdk.lorem import Resume  # noqa: F401


def test_deep_namespace_thing_reachable():
    from baml_sdk.a.b import Thing  # noqa: F401
