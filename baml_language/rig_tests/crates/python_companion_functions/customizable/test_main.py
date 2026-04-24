"""Functions with attached companion methods from BEP-030."""


def test_sync_function_is_callable():
    import baml_sdk.baml_sync as b
    assert callable(b.ExtractResume)


def test_async_function_is_callable():
    import baml_sdk.baml_async as b
    assert callable(b.ExtractResume)


def test_root_companions_attached():
    import baml_sdk.baml_sync as b
    assert callable(b.ExtractResume.build_request)
    assert callable(b.ExtractResume.render_prompt)
    assert callable(b.ExtractResume.parse)


def test_namespaced_parent_and_companion():
    import baml_sdk.baml_sync as b
    assert callable(b.foo.ClassifySentiment)
    assert callable(b.foo.ClassifySentiment.build_request)


def test_pyi_stub_exists():
    import os, baml_sdk.baml_sync as bs
    init_dir = os.path.dirname(bs.__file__)
    assert os.path.exists(os.path.join(init_dir, "__init__.pyi")), \
        "baml_sync/__init__.pyi should exist for IDE support"


def test_type_hints_resolve():
    import typing, baml_sdk.baml_sync as b
    hints = typing.get_type_hints(b.ExtractResume)
    assert "resume" in hints
