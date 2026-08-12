"""Roundtrip coverage for the symbol-collision suite — three distinct
`Bar` classes at different namespace depths plus the consumers
(`Ipsum`, `Deep`) that compose all three.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.symbol_collisions.foo import make_foo_bar, round_trip_foo_bar
from baml_sdk.symbol_collisions.fizz.foo import (
    make_fizz_foo_bar,
    round_trip_fizz_foo_bar,
)
from baml_sdk.symbol_collisions.fizz.buzz.foo import (
    make_fizz_buzz_foo_bar,
    round_trip_fizz_buzz_foo_bar,
)
from baml_sdk.symbol_collisions.lorem import make_ipsum, round_trip_ipsum
from baml_sdk.symbol_collisions.a.b.c.d import make_deep, round_trip_deep


def test_symbol_collisions_round_trip_foo_bar():
    bar = make_foo_bar(label="hi", count=2)
    assert round_trip_foo_bar(b=bar) == bar


def test_symbol_collisions_round_trip_fizz_foo_bar():
    bar = make_fizz_foo_bar(tag="t", ratio=1.5)
    assert round_trip_fizz_foo_bar(b=bar) == bar


def test_symbol_collisions_round_trip_fizz_buzz_foo_bar():
    bar = make_fizz_buzz_foo_bar(flavor="f", weight=2.5, active=True)
    assert round_trip_fizz_buzz_foo_bar(b=bar) == bar


def test_symbol_collisions_round_trip_ipsum():
    ipsum = make_ipsum(
        bar1=make_foo_bar(label="a", count=1),
        bar2=make_fizz_foo_bar(tag="b", ratio=2.0),
        bar3=make_fizz_buzz_foo_bar(flavor="c", weight=3.0, active=False),
    )
    assert round_trip_ipsum(i=ipsum) == ipsum


def test_symbol_collisions_round_trip_deep():
    ipsum = make_ipsum(
        bar1=make_foo_bar(label="a", count=1),
        bar2=make_fizz_foo_bar(tag="b", ratio=2.0),
        bar3=make_fizz_buzz_foo_bar(flavor="c", weight=3.0, active=False),
    )
    deep = make_deep(
        here=make_foo_bar(label="h", count=9),
        there=make_fizz_foo_bar(tag="th", ratio=4.0),
        further=make_fizz_buzz_foo_bar(flavor="fu", weight=5.0, active=True),
        nested=ipsum,
    )
    assert round_trip_deep(d=deep) == deep
