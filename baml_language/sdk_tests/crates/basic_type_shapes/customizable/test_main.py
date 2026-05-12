"""Per-Ty-variant type-shape coverage.

One namespace per construct family; assertions use `typing.get_type_hints`
to pin how each BAML Ty variant renders in Python. Namespaces that only
assert import-reachability today are TODO landing zones for follow-up
coverage (literal values, map shapes, union shapes, cross-class refs).
"""

import typing
import collections.abc


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


def test_all_namespaces_reachable():
    import baml_sdk
    # `ns_` prefix on the baml_src/ directory is stripped by
    # baml_project; the Python namespace is what's left.
    for ns in ("ty_variants", "literals", "maps", "complex", "unions", "defaults"):
        assert hasattr(baml_sdk, ns), f"baml_sdk.{ns} missing"


# ---------- ns_ty_variants: every Ty variant round-trips ----------

def test_ty_variants_imports_cleanly():
    from baml_sdk.ty_variants import CoverAll, MyClass, RecursiveAlias  # noqa: F401


def test_ty_variants_unknown_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert hints["unknown_field"] is typing.Any


def test_ty_variants_callable_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    origin = typing.get_origin(hints["callable_field"])
    assert origin in (typing.Callable, collections.abc.Callable)


def test_ty_variants_literal_field():
    from baml_sdk.ty_variants import CoverAll
    hints = typing.get_type_hints(CoverAll)
    assert typing.get_origin(hints["literal_field"]) is typing.Literal


def test_ty_variants_self_reference_forward_ref():
    # Self-referential class must compile even though field type references
    # the class itself before it's fully defined. Uses model_construct to
    # bypass Pydantic validation for this test — we're testing that forward
    # references resolve correctly, not value validation.
    from baml_sdk.ty_variants import CoverAll
    c = CoverAll.model_construct(
        unknown_field=None,
        callable_field=None,
        alias_field=None,
        literal_field="Hello",
        optional_nested=[],
        union_field=1,
        self_ref=None,
    )
    assert c.self_ref is None


# ---------- ns_literals: string / int / bool Literal shapes ----------

def test_literals_imports_cleanly():
    from baml_sdk.literals import Literals  # noqa: F401


# TODO: multi-value typing.Literal assertions across string / int / bool.


# ---------- ns_maps: map<K, V> shapes ----------

def test_maps_imports_cleanly():
    from baml_sdk.maps import MapContainer  # noqa: F401


# TODO: typing introspection on each map field shape
# (dict[str, int] / dict[str, dict[str, str]] / dict[str, list[str]] / etc.).


# ---------- ns_complex: 30+ classes with cross-refs and deep nesting ----------

def test_complex_imports_cleanly():
    from baml_sdk.complex import KitchenSink, UltraComplex  # noqa: F401


# TODO: cross-class reference resolution + deep-nesting smoke tests
# + stable-ordering (alphabetical by .baml source filename) assertions.


# ---------- ns_unions: primitive + class union arms ----------

def test_unions_imports_cleanly():
    from baml_sdk.unions import Container, User, Company  # noqa: F401


# TODO: typing introspection on typing.Union / typing.Optional shapes.


# ---------- ns_defaults: function arg defaults round-trip through codegen ----------
#
# The runtime contract for `_build_kwargs` / `UNSET` is unit-tested in
# `sdks/python/tests/test_sim_phase3.py`. The gap covered here is that
# `codegen_python` actually emits the corresponding
# `_define_function(..., required_positional_count)` wiring for BAML
# functions declared with default-valued parameters. We assert against
# the generated source text *and* the resulting Python signature; we
# deliberately do not invoke the BAML engine.


def test_defaults_imports_cleanly():
    from baml_sdk.defaults import (  # noqa: F401
        score,
        score_async,
        with_optional,
        with_optional_async,
        with_list,
        with_list_async,
        all_required,
        all_required_async,
        with_expr_default,
        with_expr_default_async,
        nullable_with_expr,
        nullable_with_expr_async,
    )


def test_defaults_emitted_define_function_required_count():
    """BEP-033 codegen: the 4th positional arg to `_define_function`
    is the required-positional-count. Omitted when every param is
    required (per `render_required_positional_arg`)."""
    from pathlib import Path
    import baml_sdk.defaults as defaults_mod

    source = Path(defaults_mod.__file__).read_text()
    # Partially-defaulted: 1 required positional out of 2.
    assert '"user.defaults.score", "sync",  ["query", "max_results"], 1' in source
    assert '"user.defaults.score", "async", ["query", "max_results"], 1' in source
    # Fully-defaulted: 0 required.
    assert '"user.defaults.with_optional", "sync",  ["value"], 0' in source
    assert '"user.defaults.with_list", "sync",  ["items"], 0' in source
    assert '"user.defaults.with_expr_default", "sync",  ["items"], 0' in source
    assert '"user.defaults.nullable_with_expr", "sync",  ["items"], 0' in source
    # All required: trailing count is omitted (default at the
    # define_function layer is "all required").
    assert '"user.defaults.all_required", "sync",  ["a", "b"])' in source
    assert ', 0)' not in source.split('"user.defaults.all_required"')[1].split("\n")[0]


def test_defaults_pyi_literal_defaults_project_directly():
    """BEP-033 §"sentinel problem": literal defaults (scalars, null,
    empty collections) project directly into the `.pyi` so callers see
    the real default value. Expression defaults use the stub ellipsis
    placeholder because the host can't evaluate them."""
    from pathlib import Path
    import baml_sdk.defaults as defaults_mod

    pyi = Path(defaults_mod.__file__).with_suffix(".pyi").read_text()
    # Scalar literal — visible to caller.
    assert "def score(query: str, *, max_results: int = 10) -> int" in pyi
    # Null literal — `= None`, not `_UNSET`.
    assert "def with_optional(*, value: typing.Union[str, None] = None)" in pyi
    # Empty-list literal — `= []`, not `_UNSET`.
    assert "def with_list(*, items: typing.List[int] = []) -> int" in pyi


def test_defaults_pyi_signatures_keyword_only_for_defaulted_only():
    """BEP-033 Calling Rule 1: defaulted params are keyword-only, so
    the `.pyi` must place a `*,` marker before them. Pure-required
    signatures have no marker — required params remain
    positional-compatible (BEP line 96)."""
    from pathlib import Path
    import baml_sdk.defaults as defaults_mod

    pyi = Path(defaults_mod.__file__).with_suffix(".pyi").read_text()
    # Mixed required/defaulted: marker after the required prefix.
    assert "def score(query: str, *, max_results: int = 10) -> int" in pyi
    # Fully defaulted: marker at the start of the param list.
    assert "def with_list(*, items:" in pyi
    # No `*,` for the all-required function.
    assert "def all_required(a: int, b: int) -> int" in pyi
    assert "def all_required(a: int, *, b" not in pyi


def test_defaults_pyi_expression_defaults_render_as_ellipsis():
    """BEP-033 §"sentinel problem" Python row: runtime expression
    defaults use a sentinel, but `.pyi` signatures use the PEP 484
    ellipsis placeholder so the public parameter type stays clean."""
    from pathlib import Path
    import baml_sdk.defaults as defaults_mod

    pyi = Path(defaults_mod.__file__).with_suffix(".pyi").read_text()
    # The runtime sentinel should not leak into stubs.
    assert "_UNSET" not in pyi
    assert "from baml_core import UNSET" not in pyi
    # Non-empty array default lowers to `...`, not `[1, 2, 3]`.
    assert "def with_expr_default(*, items: typing.List[int] = ...) -> int" in pyi
    # Nullable + expression default — BEP §"Nullable params with
    # expression defaults" (the "trickiest case"). Runtime still uses
    # the sentinel to distinguish "not passed" from `None`; the stub
    # uses ellipsis to mark the parameter as defaulted.
    assert (
        "def nullable_with_expr(*, items: typing.Union[typing.List[int], None] = ...)"
        in pyi
    )


def test_defaults_runtime_signature_rejects_extra_positionals():
    """BEP-033 Calling Rule 1: a defaulted param can't be passed
    positionally. The runtime wrapper consults
    `required_positional_count` to enforce this — extra positionals
    raise TypeError before the engine is touched."""
    import pytest
    from baml_sdk.defaults import score, with_optional, all_required

    # `score` has 1 required positional; passing 2 must raise BEFORE
    # the runtime tries to dispatch.
    with pytest.raises(TypeError):
        score("cats", 5)

    # `with_optional` has 0 required positionals; any positional is
    # too many.
    with pytest.raises(TypeError):
        with_optional("nope")

    # `all_required` accepts 2 positionals; 3 is too many. We can't
    # call with 2 here (no engine), so just pin the "too many"
    # boundary via 3 positionals.
    with pytest.raises(TypeError):
        all_required(1, 2, 3)


def test_defaults_runtime_accepts_valid_call_shapes():
    """BEP-033 line 96: required params accept positional OR named.
    BEP-033 Calling Rule 1 positive side: defaulted params can be
    omitted or passed by name. We don't run the engine — engine-side
    errors are tolerated; we only assert no `TypeError` from the
    argument-binding layer."""
    from baml_sdk.defaults import score, with_optional, with_list, all_required

    def call_binding_ok(fn, *args, **kwargs):
        """Invoke `fn` and assert it did not raise `TypeError` from
        the Python-side argument binding. Any *other* exception
        (engine dispatch, runtime not initialized, etc.) is treated
        as "binding succeeded, then something else happened" and
        passes the test."""
        try:
            fn(*args, **kwargs)
        except TypeError as e:
            raise AssertionError(
                f"argument binding rejected a BEP-valid call: "
                f"fn={fn!r} args={args!r} kwargs={kwargs!r}: {e}"
            )
        except Exception:
            pass  # engine-level failure is out of scope

    # `score` — required can be positional or named; defaulted only named.
    call_binding_ok(score, "cats")                            # required positional
    call_binding_ok(score, query="cats")                      # required named
    call_binding_ok(score, "cats", max_results=5)             # mixed
    call_binding_ok(score, query="cats", max_results=5)       # both named

    # Defaulted-only — omit, or pass by name.
    call_binding_ok(with_optional)
    call_binding_ok(with_optional, value="hi")
    call_binding_ok(with_optional, value=None)                # explicit None ≠ omitted
    call_binding_ok(with_list)
    call_binding_ok(with_list, items=[1, 2])

    # All-required — both positional and full-named are valid.
    call_binding_ok(all_required, 1, 2)
    call_binding_ok(all_required, a=1, b=2)
    call_binding_ok(all_required, 1, b=2)                     # mix


def test_defaults_runtime_rejects_defaulted_passed_positionally():
    """BEP-033 Calling Rule 1 (negative): defaulted params *must* be
    named. `score` has the defaulted `max_results` at position 1; the
    BEP forbids `Search("cats", 5)` (line 113)."""
    import pytest
    from baml_sdk.defaults import score, with_expr_default

    with pytest.raises(TypeError):
        score("cats", 5)
    # Same rule applies whether the default is a literal or an
    # expression — codegen-side they're both keyword-only.
    with pytest.raises(TypeError):
        with_expr_default([1, 2])


def test_defaults_calling_rule_2_no_positional_after_named():
    """BEP-033 Calling Rule 2 (line 94): once an arg is named, every
    subsequent arg must also be named. In Python this is a syntax
    error at parse time (`SyntaxError: positional argument follows
    keyword argument`), so codegen doesn't need to enforce it — but
    pinning it here documents that Python provides this rule for free."""
    import ast as py_ast
    import pytest

    # Direct Python source — Python rejects this at parse time, before
    # the wrapped function is ever consulted.
    with pytest.raises(SyntaxError):
        py_ast.parse('all_required(a=1, 2)')
