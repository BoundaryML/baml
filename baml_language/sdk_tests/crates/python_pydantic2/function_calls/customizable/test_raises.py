"""32d — throws-contract `Raises:` docstring coverage.

Pins how `sdkgen_python_pydantic2` renders the inferred throws contract
(`callable_throws`) into a Google-style `Raises:` block on generated
functions' docstrings, using **unqualified** type names. The runtime
`__doc__` (read by `inspect.getdoc`) is driven by the `.py` `__doc__ =`
trailer for free functions; methods carry their `Raises:` in the `.pyi`
only (the 32d decision), so the method case asserts on the stub source.
"""

import inspect


def test_raises_imports_symbols_reachable():
    import baml_sdk  # noqa: F401
    from baml_sdk.raises_test import (  # noqa: F401
        DocLoader,
        InferredThrow,
        LoadDoc,
        PureLen,
        Reparse,
    )


def test_raises_union_throws_lists_all_names():
    from baml_sdk.raises_test import LoadDoc

    doc = inspect.getdoc(LoadDoc)
    assert doc is not None
    assert doc.rstrip().endswith("Raises:\n    ParseError, TimeoutError"), repr(doc)


def test_raises_async_sibling_also_has_raises():
    from baml_sdk.raises_test import LoadDoc_async

    doc = inspect.getdoc(LoadDoc_async)
    assert doc is not None
    assert doc.rstrip().endswith("Raises:\n    ParseError, TimeoutError"), repr(doc)


def test_raises_single_throws():
    from baml_sdk.raises_test import Reparse

    doc = inspect.getdoc(Reparse)
    assert doc is not None
    assert doc.rstrip().endswith("Raises:\n    ParseError"), repr(doc)


def test_raises_summary_precedes_raises_block():
    from baml_sdk.raises_test import LoadDoc

    doc = inspect.getdoc(LoadDoc)
    assert doc is not None
    assert doc.startswith("Load a document from a path."), repr(doc)
    assert "\n\nRaises:\n" in doc, repr(doc)


def test_raises_inferred_contract_without_clause_still_raises():
    # No written `throws` clause, but the body throws ParseError — the
    # inferred contract (callable_throws) still surfaces a Raises block.
    from baml_sdk.raises_test import InferredThrow

    doc = inspect.getdoc(InferredThrow)
    assert doc is not None
    assert doc.rstrip().endswith("Raises:\n    ParseError"), repr(doc)


def test_raises_non_throwing_function_has_no_raises_block():
    from baml_sdk.raises_test import PureLen

    doc = inspect.getdoc(PureLen) or ""
    assert "Raises:" not in doc, repr(doc)


def test_raises_method_raises_block_in_pyi():
    # Methods carry `Raises:` in the .pyi (pyright/IDE surface) per the 32d
    # decision; the runtime `.py` __doc__ trailer is free-functions-only.
    import baml_sdk.raises_test as mod

    pyi_src = open(mod.__file__ + "i").read()  # __init__.py -> __init__.pyi

    load_block = _def_block(pyi_src, "def load(")
    assert "Raises:" in load_block and "ParseError" in load_block, load_block

    create_block = _def_block(pyi_src, "def create(")
    assert "Raises:" in create_block and "TimeoutError" in create_block, create_block


def _def_block(src: str, marker: str) -> str:
    """The `.pyi` source from `marker` up to the next def/decorator line."""
    start = src.index(marker)
    rest = src[start + len(marker) :]
    # Stop at the next sibling member (a `def`/`async def`/`@staticmethod`).
    ends = [
        rest.find("\n    def "),
        rest.find("\n    async def "),
        rest.find("\n    @staticmethod"),
    ]
    ends = [e for e in ends if e != -1]
    stop = min(ends) if ends else len(rest)
    return marker + rest[:stop]
