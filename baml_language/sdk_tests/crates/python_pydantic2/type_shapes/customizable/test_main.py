"""Smoke tests for the type_shapes sdk-test crate.

The actual type-shape verification happens in `pyright baml_sdk` (run by
the `pyright` test in `tests/sdk_test.rs`). These pytest cases just
confirm each generated namespace imports cleanly and that the symbols
listed in 18a are reachable.
"""


def test_main_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_main_all_namespaces_reachable():
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
    import baml_sdk.complex_models  # noqa: F401
    import baml_sdk.lorem  # noqa: F401
    import baml_sdk.a  # noqa: F401


def test_main_root_foo_reachable():
    from baml_sdk import Foo  # noqa: F401


def test_main_lorem_resume_reachable():
    from baml_sdk.lorem import Resume  # noqa: F401


def test_main_deep_namespace_thing_reachable():
    from baml_sdk.a.b import Thing  # noqa: F401


def test_main_generated_models_ignore_extra_fields():
    from baml_sdk.generics import Wrapper
    from baml_sdk.lorem import Resume
    from baml_sdk.stream_types.lorem import Resume as StreamResume

    resume = Resume.model_validate(
        {"name": "Ada", "email": None, "future_field": "ignored"}
    )
    wrapper = Wrapper[int].model_validate({"value": 42, "future_field": "ignored"})
    stream_resume = StreamResume.model_validate(
        {"name": "Grace", "email": None, "future_field": "ignored"}
    )

    assert resume.model_dump() == {"name": "Ada", "email": None}
    assert wrapper.model_dump() == {"value": 42}
    assert stream_resume.model_dump() == {"name": "Grace", "email": None}

    for model in (resume, wrapper, stream_resume):
        assert model.model_config["extra"] == "ignore"
        assert model.model_extra is None
