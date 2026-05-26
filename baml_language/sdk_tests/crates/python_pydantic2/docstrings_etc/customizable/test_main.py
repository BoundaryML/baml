"""BAML `///` doc-comment lowering coverage.

Asserts on the runtime `__doc__` of generated classes and enums so the
rules in `ns_docs/types.baml` are pinned end-to-end (CST → AST → HIR →
IR → codegen → import).
"""


def test_imports():
    import baml_sdk  # noqa: F401
    import baml_sdk.docs  # noqa: F401
    from baml_sdk.docs import Doc, Note, Priority, Sentiment  # noqa: F401


def test_class_doc_summary_and_attributes_section_in_dunder_doc():
    import inspect

    from baml_sdk.docs import Doc

    # `inspect.getdoc` normalizes leading/trailing whitespace and dedents
    # so the assertions are stable against the body indent.
    doc = inspect.getdoc(Doc)
    assert doc is not None
    expected = (
        "A document with a title and an optional body.\n"
        "\n"
        "Attributes:\n"
        "    title: Title shown in lists and search results.\n"
        "    body: Free-form body text."
    )
    assert doc == expected, f"got:\n{doc!r}"


def test_undocumented_field_listed_as_bare_name_under_attributes():
    # Note: `id` is documented, `text` is not. The "any-doc" rule says
    # the Attributes: section appears (because `id` carries a `///`)
    # and lists every field, with `text` rendered as a bare name.
    import inspect

    from baml_sdk.docs import Note

    doc = inspect.getdoc(Note)
    assert doc is not None
    assert doc.startswith("A multi-line summary.\nContinuation line")
    assert "\n\nAttributes:\n    id: Stable identifier" in doc
    # Bare-name entry: just the identifier, no trailing colon, no doc.
    assert doc.rstrip().endswith("\n    text"), f"got:\n{doc!r}"


def test_enum_doc_summary_and_members_section_in_dunder_doc():
    import inspect

    from baml_sdk.docs import Sentiment

    doc = inspect.getdoc(Sentiment)
    assert doc is not None
    expected = (
        "Sentiment labels surfaced by the model.\n"
        "\n"
        "Members:\n"
        "    HAPPY: Smiling face.\n"
        "    SAD: Frowning face.\n"
        "    NEUTRAL"
    )
    assert doc == expected, f"got:\n{doc!r}"


def test_enum_summary_only_omits_members_section_when_no_variant_documented():
    # Priority has a class-level /// but no variant carries one — the
    # Members: section should be suppressed entirely. Variants are
    # still importable / iterable normally.
    import inspect

    from baml_sdk.docs import Priority

    doc = inspect.getdoc(Priority)
    assert doc is not None
    assert doc == (
        'Pin the "summary only, no member rollup" case: this enum has a\n'
        "class-level `///` but every variant is bare."
    ), f"got:\n{doc!r}"
    assert "Members:" not in doc, f"Members: section leaked in:\n{doc}"
    assert {m.value for m in Priority} == {"HIGH", "MEDIUM", "LOW"}


def test_no_inline_field_or_variant_doc_artifacts():
    # Field/variant `///` lines must not produce inline `# …` comments
    # or `"""…"""` attribute docstrings — they live exclusively inside
    # the parent's Attributes:/Members: section.
    import importlib

    mod = importlib.import_module("baml_sdk.docs")
    with open(mod.__file__) as f:
        src = f.read()
    assert "# Title shown in lists" not in src, src
    assert "# Smiling face" not in src, src
    # Triple-quoted strings only ever come in pairs (open + close), one
    # per parent that emits a docstring. Doc/Note/Sentiment/Priority
    # all have one each → 4 docstrings × 2 fences = 8.
    triples = src.count('"""')
    assert triples == 8, f"unexpected triple-quote count: {triples}\n{src}"
