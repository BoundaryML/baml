"""Roundtrip coverage for `baml_sdk.literals` — literal Ty variants.

Float literals are intentionally absent (Python `Literal` rejects
floats). The negative-literal-as-field case is still parser-blocked, but
the function-return form `return_literal_neg_one() -> -1` emits and is
exercised here.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.literals import (
    Literals,
    return_literal42_sync,
    return_literal_neg_one_sync,
    return_literal_draft_sync,
    return_literal_escaped_sync,
    return_literal_true_sync,
    return_literal_false_sync,
    round_trip_literal42_sync,
    round_trip_literal_draft_sync,
    round_trip_literal_escaped_sync,
    round_trip_literal_true_sync,
    round_trip_literal_false_sync,
    round_trip_literals_sync,
)


def test_literals_return_literals():
    assert return_literal42_sync() == 42
    assert return_literal_neg_one_sync() == -1
    assert return_literal_draft_sync() == "draft"
    assert return_literal_escaped_sync() == 'has "quotes"'
    assert return_literal_true_sync() is True
    assert return_literal_false_sync() is False


def test_literals_round_trip_literal42():
    assert round_trip_literal42_sync(x=42) == 42


def test_literals_round_trip_literal_draft():
    assert round_trip_literal_draft_sync(x="draft") == "draft"


def test_literals_round_trip_literal_escaped():
    assert round_trip_literal_escaped_sync(x='has "quotes"') == 'has "quotes"'


def test_literals_round_trip_literal_true():
    assert round_trip_literal_true_sync(x=True) is True


def test_literals_round_trip_literal_false():
    assert round_trip_literal_false_sync(x=False) is False


def test_literals_round_trip_literals():
    lit = Literals(
        literal_42=42,
        literal_draft="draft",
        literal_escaped='has "quotes"',
        literal_true=True,
        literal_false=False,
    )
    assert round_trip_literals_sync(l=lit) == lit


def test_literals_round_trip_flag_mixed_literal_union():
    from baml_sdk.literals import round_trip_flag_sync

    assert round_trip_flag_sync(f="active") == "active"
    r_int = round_trip_flag_sync(f=1)
    assert r_int == 1 and not isinstance(r_int, bool)
    r_bool = round_trip_flag_sync(f=True)
    assert r_bool is True
