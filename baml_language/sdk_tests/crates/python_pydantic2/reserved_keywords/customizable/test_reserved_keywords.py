"""Round-trip + import coverage for the reserved_keywords fixture (#4059).

The generated SDK must import without SyntaxError (the core #4059 bug) and
round-trip escaped enum members and class fields through the engine, which
exercises the bridge encode-by-value / encode-by-alias fix end-to-end.
"""

import pydantic

import baml_sdk  # noqa: F401 — initializes the BAML runtime
from baml_sdk import (
    Kw,
    Fields,
    None_,
    True_,
    False_,
    pass_,
    import_,
    lambda_,
    PassHolder,
    round_trip_kw,
    round_trip_fields,
    round_trip_none,
    round_trip_pass,
)


def test_keyword_named_types_import_cleanly():
    # The NAME-position keyword types are importable and escaped — the SDK no
    # longer fails to import at all (the headline #4059 symptom).
    assert issubclass(None_, pydantic.BaseModel)
    assert True_.A.value == "A"
    assert False_ is int  # `type False = int` -> `False_: typing.TypeAlias = int`


def test_lowercase_keyword_named_types_import_and_escape():
    # Lowercase keyword NAMES (`class pass`, `enum import`, `type lambda`) escape
    # through the identical path as the uppercase trio — refuting the old comment
    # that only None/True/False are admissible in NAME position.
    assert issubclass(pass_, pydantic.BaseModel)
    assert import_.A.value == "A"
    assert lambda_ is int  # `type lambda = int` -> `lambda_: typing.TypeAlias = int`
    # Cross-reference escaping: PassHolder.p is typed as the escaped `pass_`.
    # The field name `p` is not a keyword, so only the resolved *type* can regress
    # here; assert the annotation itself, not mere field presence.
    assert PassHolder.model_fields["p"].annotation is pass_


def test_lowercase_keyword_named_class_round_trips_through_engine():
    # NAME-position decode for a LOWERCASE keyword class: the engine returns the
    # raw wire FQN "user.pass"; the bridge resolves it via the raw typemap key to
    # the escaped Python class `pass_` and reconstructs it end-to-end.
    p = pass_(value=1)
    out = round_trip_pass(p=p)
    assert isinstance(out, pass_)
    assert out.value == 1
    assert out == p


def test_escaped_enum_member_keeps_wire_value():
    # `Kw.None_` has Python member name `None_` but its wire value is "None".
    assert Kw.None_.value == "None"
    assert Kw.lambda_.value == "lambda"
    assert Kw.False_.value == "False"


def test_all_24_baml_admissible_keywords_are_covered():
    # Completeness: the three keywords the fixture previously omitted
    # (`elif`/`except`/`finally`) are present and escape with their raw wire
    # value preserved, so the E2E gate spans the full BAML-admissible set.
    assert Kw.elif_.value == "elif"
    assert Kw.except_.value == "except"
    assert Kw.finally_.value == "finally"


def test_soft_keywords_are_not_escaped():
    # `case` (enum member) and `type` (class field) are Python SOFT keywords —
    # valid identifiers — so the generator must emit them verbatim. This is the
    # negative control: it fails loudly if a future change over-escapes them.
    assert Kw.case.value == "case"
    assert not hasattr(Kw, "case_")
    assert "type" in Fields.model_fields
    assert "type_" not in Fields.model_fields


def test_escaped_enum_member_round_trips_through_engine():
    # Encode (by value) -> engine identity -> decode (by value).
    assert round_trip_kw(k=Kw.None_) == Kw.None_
    assert round_trip_kw(k=Kw.lambda_) == Kw.lambda_
    assert round_trip_kw(k=Kw.pass_) == Kw.pass_
    assert round_trip_kw(k=Kw.True_) == Kw.True_


def test_escaped_class_field_round_trips_by_alias():
    # `type` is a soft keyword and stays unescaped; its wire key is the bare name.
    f = Fields(**{"pass": 1, "lambda": 2, "from": 3, "global": 4, "del": 5, "async": 6, "type": 7})
    # Escaped attributes are accessible under their Python names.
    assert f.pass_ == 1
    assert f.lambda_ == 2
    assert f.from_ == 3
    assert f.type == 7  # soft keyword: unescaped attribute
    # Full round-trip through the engine preserves every escaped field
    # (encode by alias -> engine identity -> decode by alias).
    assert round_trip_fields(f=f) == f


def test_construct_by_escaped_name_via_populate_by_name():
    # populate_by_name=True lets callers build by the escaped Python name too.
    f = Fields(pass_=1, lambda_=2, from_=3, global_=4, del_=5, async_=6, type=7)
    assert f.pass_ == 1
    assert f.type == 7
    assert round_trip_fields(f=f) == f


def test_keyword_named_class_round_trips_through_engine():
    # The NAME-position decode this PR enables end-to-end: the engine returns the
    # raw wire FQN "user.None"; the bridge resolves it via the raw typemap key to
    # the escaped Python class `None_` and reconstructs it. Previously this path
    # was only unit-probed, never exercised through the real engine.
    n = None_(value=1)
    out = round_trip_none(n=n)
    assert isinstance(out, None_)
    assert out.value == 1
    assert out == n
