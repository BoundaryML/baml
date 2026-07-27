"""Roundtrip coverage for `baml_sdk.literals` — literal Ty variants.

Float literals are intentionally absent (Python `Literal` rejects
floats). The negative-literal-as-field case is still parser-blocked, but
the function-return form `return_literal_neg_one() -> -1` emits and is
exercised here.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.literals import (
    Literals,
    return_literal42,
    return_literal_neg_one,
    return_literal_draft,
    return_literal_escaped,
    return_literal_true,
    return_literal_false,
    round_trip_literal42,
    round_trip_literal_draft,
    round_trip_literal_escaped,
    round_trip_literal_true,
    round_trip_literal_false,
    round_trip_literals,
)


def test_literals_return_literals():
    assert return_literal42() == 42
    assert return_literal_neg_one() == -1
    assert return_literal_draft() == "draft"
    assert return_literal_escaped() == 'has "quotes"'
    assert return_literal_true() is True
    assert return_literal_false() is False


def test_literals_round_trip_literal42():
    assert round_trip_literal42(x=42) == 42


def test_literals_round_trip_literal_draft():
    assert round_trip_literal_draft(x="draft") == "draft"


def test_literals_round_trip_literal_escaped():
    assert round_trip_literal_escaped(x='has "quotes"') == 'has "quotes"'


def test_literals_round_trip_literal_true():
    assert round_trip_literal_true(x=True) is True


def test_literals_round_trip_literal_false():
    assert round_trip_literal_false(x=False) is False


def test_literals_round_trip_literals():
    lit = Literals(
        literal_42=42,
        literal_draft="draft",
        literal_escaped='has "quotes"',
        literal_true=True,
        literal_false=False,
    )
    assert round_trip_literals(l=lit) == lit


def test_literals_round_trip_flag_mixed_literal_union():
    from baml_sdk.literals import round_trip_flag

    assert round_trip_flag(f="active") == "active"
    r_int = round_trip_flag(f=1)
    assert r_int == 1 and not isinstance(r_int, bool)
    r_bool = round_trip_flag(f=True)
    assert r_bool is True
