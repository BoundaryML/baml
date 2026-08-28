"""End-to-end check of the 09a-style baml_src → baml_sdk pipeline.

Drives codegen from real `.baml` source through the full
`baml_project::build_symbol_pool` path (parse → HIR → TIR → SymbolPool
→ emitter).

Scope (subset of 09a-codegen-example-scenario.md):
- user.lorem.Resume + ExtractResume (with flat spec/stream projections)
- user.lorem.StreamingDoc + StreamingExtract (pins the always-different
  `$stream` companion branch; folded in from the former
  `python_llm_functions` crate)
- user.ipsum.Sentiment (enum) + ClassifySentiment
- stream_types/lorem leaf presence
"""


def test_main_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_main_namespaces_reachable_via_explicit_import():
    import baml_sdk.lorem  # noqa: F401
    import baml_sdk.ipsum  # noqa: F401


def test_main_lorem_resume_class_shape():
    import pydantic
    from baml_sdk.lorem import Resume

    assert issubclass(Resume, pydantic.BaseModel)
    fields = Resume.model_fields
    assert set(fields) == {"name", "email"}


# SDK_PARITY_LINT(skip): pins the Python generator's nullable-field `= None` default
def test_main_nullable_model_field_can_be_omitted():
    from baml_sdk.lorem import Resume

    resume = Resume(name="Ada")
    assert resume.email is None


def test_main_lorem_streaming_doc_class_shape():
    import pydantic
    from baml_sdk.lorem import StreamingDoc

    assert issubclass(StreamingDoc, pydantic.BaseModel)
    assert set(StreamingDoc.model_fields) == {"title", "body", "word_count"}


def test_main_ipsum_sentiment_enum_shape():
    import enum
    from baml_sdk.ipsum import Sentiment

    assert issubclass(Sentiment, enum.Enum)
    assert {v.name for v in Sentiment} == {"POSITIVE", "NEGATIVE", "NEUTRAL"}
    # 09b: enum values are the variant name verbatim, mixed with `str`
    # so they round-trip through JSON.
    assert Sentiment.POSITIVE.value == "POSITIVE"
    assert isinstance(Sentiment.POSITIVE, str)


def test_main_extract_resume_factory_bindings():
    # Sync + async siblings at the namespace leaf, per 09b §4.
    from baml_sdk import lorem

    assert callable(lorem.ExtractResume)
    assert callable(lorem.ExtractResume_async)


def test_main_extract_resume_operation_bindings():
    from baml_sdk import lorem

    for name in (
        "ExtractResume_spec",
        "ExtractResume_spec_async",
        "ExtractResume_stream",
        "ExtractResume_stream_async",
    ):
        binding = getattr(lorem, name)
        assert callable(binding), f"missing operation binding {name}"

    for removed in (
        "ExtractResume__render_prompt",
        "ExtractResume__render_prompt_async",
        "ExtractResume__build_request",
        "ExtractResume__build_request_async",
        "ExtractResume__parse",
        "ExtractResume__parse_async",
    ):
        assert not hasattr(lorem, removed), f"obsolete companion leaked: {removed}"


def test_main_streaming_extract_factory_bindings():
    from baml_sdk import lorem

    assert callable(lorem.StreamingExtract)
    assert callable(lorem.StreamingExtract_async)


def test_main_streaming_extract_operation_bindings():
    from baml_sdk import lorem

    for name in (
        "StreamingExtract_spec",
        "StreamingExtract_spec_async",
        "StreamingExtract_stream",
        "StreamingExtract_stream_async",
    ):
        binding = getattr(lorem, name)
        assert callable(binding), f"missing operation binding {name}"

    for removed in (
        "StreamingExtract__render_prompt",
        "StreamingExtract__render_prompt_async",
        "StreamingExtract__build_request",
        "StreamingExtract__build_request_async",
        "StreamingExtract__parse",
        "StreamingExtract__parse_async",
    ):
        assert not hasattr(lorem, removed), f"obsolete companion leaked: {removed}"


def test_main_stream_types_lorem_leaf_present():
    # PPIR synthesizes Class$stream companions for any class referenced
    # by an LLM function's return type. Both Resume and StreamingDoc
    # are LLM return types, so `stream_types.lorem` must exist with at
    # least one of them. StreamingDoc's conditional-emit outcome is
    # pinned to "emitted" (its `body string?` proxies a
    # stream-state-altering field); Resume's is left to the
    # conditional-emit rule.
    from baml_sdk.stream_types import lorem as stream_lorem

    has_any = any(hasattr(stream_lorem, name) for name in ("Resume", "StreamingDoc"))
    assert has_any, (
        "expected at least one $stream companion class in stream_types/lorem"
    )


# SDK_PARITY_LINT(skip): validates Python generated stream_types package imports
def test_main_nested_stream_partial_module_imports_cleanly():
    from baml_sdk.stream_types import stream_typing

    assert stream_typing.TextResultStreamHolder is not None


def test_main_classify_sentiment_factory_bindings():
    from baml_sdk import ipsum

    assert callable(ipsum.ClassifySentiment)
    assert callable(ipsum.ClassifySentiment_async)


# ---------------------------------------------------------------------------
# Replay-harness surface (bridge-generics/streaming/02). Codegen-shape only;
# the keyless behavioral tests live in `test_streaming_e2e.py` (string- and
# class-typed `T`). The replay client is the unified env-driven `StreamStub`
# (no separate Replay* functions).
# ---------------------------------------------------------------------------


def test_main_replay_server_namespace_bindings():
    # The BAML-implemented replay server lives in the `replay` namespace
    # (ns_replay/). Both invocation entry points get sync + async siblings.
    from baml_sdk import replay

    for name in (
        "replay_serve_until_shutdown",
        "replay_serve_until_shutdown_async",
        "replay_serve_detached",
        "replay_serve_detached_async",
    ):
        binding = getattr(replay, name)
        assert callable(binding), f"missing replay-server binding {name}"


# NOTE: the shorthand-client api_key wiring tests that lived here inspected
# the auth header on `*$build_request`'s Request. That companion went away
# with the legacy LLM path (credentials now resolve inside the provider's
# `invoke`, at request time), so there is no pre-network Request to inspect.
# Coverage moved to the live smokes in `_planv2/baml_src/live/`.
