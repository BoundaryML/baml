"""Host-call coverage for optional (default-valued) function parameters.

`main.baml` declares two functions with BAML-side argument defaults:

    function scale(base: int, factor: int = 2) -> int { base * factor }
    function classify(value: int? = 7) -> int { ... }

Codegen renders each defaulted parameter as keyword-only with a Python
default (`scale(base, *, factor=2)`), and threads a
`required_positional_count` into `_define_function`. At call time the
generated binding only encodes the arguments the caller actually supplied
(`_build_kwargs` drops the rest), so an omitted default is filled in by the
engine — *not* by Python — and an omitted argument is distinct from an
explicit `None`.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import classify, scale


def test_scale_uses_engine_default_when_factor_omitted():
    # `factor` omitted → engine substitutes the BAML default `2`.
    assert scale(5) == 10


def test_scale_override_default_by_keyword():
    # `factor` is keyword-only (codegen inserts the `*` marker), so the
    # override must be passed by name.
    assert scale(5, factor=3) == 15


def test_classify_omitted_uses_default_seven():
    # No `value` argument → engine fills the default `7`, taking the
    # non-null branch.
    assert classify() == 7


def test_classify_explicit_null_is_distinct_from_omitted():
    # Explicit `None` is encoded as a real `null` arg → the null branch
    # returns -1, distinguishing it from the omitted-default case above.
    assert classify(value=None) == -1


def test_classify_supplied_value_passes_through():
    assert classify(value=5) == 5
