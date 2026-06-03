"""Smoke tests for plain (non-LLM) expression functions.

Covers the nullary base case, a single required argument, and the full matrix
of call forms for a function with required + optional (default-valued)
parameters.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk import (
    hello_world,
    required_with_optional_args,
    single_required_arg,
)


def test_hello_world_returns_literal():
    assert hello_world() == "hello world"


def test_single_required_arg_round_trips():
    # The next step up from the nullary case: one required positional
    # argument round-trips through the engine unchanged.
    assert single_required_arg("hi") == "hi"


# ── required_with_optional_args(arg0: int, opt1: int? = 5, opt2: int? = make_opt2()) ──
#
# `void` return → None on the host, so each call just asserts `is None`; this
# fixture models the *call forms*, not a computed value. `opt1` has a literal
# default; `opt2`'s default is an expression (`make_opt2()`) the engine
# evaluates when `opt2` is omitted.
#
# Codegen emits the two defaulted params as keyword-only (a `*` marker) with
# `required_positional_count = 1`, so in Python the optionals are passed by
# name, never positionally.
#
#   BAML call form                                 │ Python equivalent
#   ───────────────────────────────────────────────┼─────────────────────────────────────
#   required_with_optional_args(1)                 │ required_with_optional_args(1)
#   required_with_optional_args(arg0 = 1)          │ required_with_optional_args(arg0=1)
#   required_with_optional_args(1, opt1 = 2)       │ required_with_optional_args(1, opt1=2)
#   required_with_optional_args(1, opt2 = 3)       │ required_with_optional_args(1, opt2=3)
#   required_with_optional_args(1, opt1 = 2, opt2 = 3) │ required_with_optional_args(1, opt1=2, opt2=3)
#   required_with_optional_args(1, opt1 = null)    │ required_with_optional_args(1, opt1=None)
#   required_with_optional_args(1, opt2 = null)    │ required_with_optional_args(1, opt2=None)
#   required_with_optional_args(1, 2)              │ TODO — see below (positional optional)
#   required_with_optional_args(1, 2, 3)           │ TODO — see below (positional optional)
#   required_with_optional_args(1, null, null)     │ TODO — see below (positional optional)
#
# TODO(sdkgen_python_pydantic2): BAML lets optionals be passed positionally
# (`(1, 2, 3)`), but the generated binding makes them keyword-only — both the
# `*` marker and `required_positional_count = 1` reject `f(1, 2)`. Decide whether
# positional optionals should be supported; if so, codegen must drop the `*`
# marker and widen `required_positional_count`. The keyword forms above already
# exercise the same engine call, so this is a surface-ergonomics gap, not a
# coverage hole.


def test_call_required_only_fills_both_defaults():
    # opt1 → 5 (literal default), opt2 → make_opt2() (expression default).
    assert required_with_optional_args(1) is None


def test_call_required_by_name():
    assert required_with_optional_args(arg0=1) is None


def test_call_named_opt1_only():
    assert required_with_optional_args(1, opt1=2) is None


def test_call_named_opt2_only_skips_opt1():
    # opt2 set by name while opt1 falls back to its literal default 5.
    assert required_with_optional_args(1, opt2=3) is None


def test_call_both_optionals_by_name():
    assert required_with_optional_args(1, opt1=2, opt2=3) is None


def test_call_explicit_null_opt1():
    assert required_with_optional_args(1, opt1=None) is None


def test_call_explicit_null_opt2():
    assert required_with_optional_args(1, opt2=None) is None
