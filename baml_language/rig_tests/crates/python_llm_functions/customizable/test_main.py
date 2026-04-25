"""End-to-end check of the LLM-function emission path.

Drives codegen from real `.baml` source through the full
`baml_project::build_symbol_pool` pipeline. Asserts:
- Sync + async siblings for each LLM function.
- All four auto-generated companions per LLM function (`__build_request`,
  `__render_prompt`, `__parse`, `__parse_stream`) — sync + async.
- `param_names` introspection from `define_function`.
- `$stream` companion-class emission under `stream_types/` (conditional
  on whether streaming expansion alters the source class).
"""


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_user_namespace_reachable():
    import baml_sdk
    assert baml_sdk.user is not None


def test_resume_class_shape():
    import pydantic
    from baml_sdk.user import Resume

    assert issubclass(Resume, pydantic.BaseModel)
    assert set(Resume.model_fields) == {"name", "email"}


def test_streaming_doc_class_shape():
    import pydantic
    from baml_sdk.user import StreamingDoc

    assert issubclass(StreamingDoc, pydantic.BaseModel)
    assert set(StreamingDoc.model_fields) == {"title", "body", "word_count"}


def test_extract_resume_factory_bindings():
    from baml_sdk import user

    assert callable(user.ExtractResume)
    assert callable(user.ExtractResume_async)
    assert user.ExtractResume.param_names == ["text"]


def test_streaming_extract_factory_bindings():
    from baml_sdk import user

    assert callable(user.StreamingExtract)
    assert callable(user.StreamingExtract_async)
    assert user.StreamingExtract.param_names == ["text"]


def test_extract_resume_companion_bindings():
    from baml_sdk import user

    for name in (
        "ExtractResume__build_request",
        "ExtractResume__render_prompt",
        "ExtractResume__parse",
        "ExtractResume__parse_stream",
    ):
        binding = getattr(user, name)
        assert callable(binding), f"missing companion binding {name}"
        binding_async = getattr(user, f"{name}_async")
        assert callable(binding_async), f"missing async companion {name}_async"


def test_streaming_extract_companion_bindings():
    from baml_sdk import user

    for name in (
        "StreamingExtract__build_request",
        "StreamingExtract__render_prompt",
        "StreamingExtract__parse",
        "StreamingExtract__parse_stream",
    ):
        binding = getattr(user, name)
        assert callable(binding), f"missing companion binding {name}"
        binding_async = getattr(user, f"{name}_async")
        assert callable(binding_async), f"missing async companion {name}_async"


def test_stream_types_namespace_present():
    # PPIR synthesizes Class$stream companions for any class referenced
    # by an LLM function's return type. Both Resume and StreamingDoc
    # are LLM return types, so `stream_types.user` must exist with
    # at least one of them.
    from baml_sdk import stream_types

    assert hasattr(stream_types, "user")
    leaf = stream_types.user
    # At least one of Resume / StreamingDoc has a $stream companion;
    # the conditional-emit rule decides which. The test asserts the
    # leaf module exists, leaving conditional-emit shape to the snapshot.
    has_any = any(hasattr(leaf, name) for name in ("Resume", "StreamingDoc"))
    assert has_any, "expected at least one $stream companion class in stream_types/user"


def test_inlinedbaml_files_present():
    from baml_sdk.baml import _inlinedbaml

    paths = set(_inlinedbaml.FILES.keys())
    assert "ns_user/types.baml" in paths
    assert "ns_user/functions.baml" in paths
