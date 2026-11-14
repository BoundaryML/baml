"""Roundtrip coverage for the symbol-collision suite — three distinct
`Bar` classes at different namespace depths plus the consumers
(`Ipsum`, `Deep`) that compose all three.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.symbol_collisions.foo import make_foo_bar_sync, round_trip_foo_bar_sync
from baml_sdk.symbol_collisions.fizz.foo import (
    make_fizz_foo_bar_sync,
    round_trip_fizz_foo_bar_sync,
)
from baml_sdk.symbol_collisions.fizz.buzz.foo import (
    make_fizz_buzz_foo_bar_sync,
    round_trip_fizz_buzz_foo_bar_sync,
)
from baml_sdk.symbol_collisions.lorem import make_ipsum_sync, round_trip_ipsum_sync
from baml_sdk.symbol_collisions.a.b.c.d import make_deep_sync, round_trip_deep_sync


def test_symbol_collisions_round_trip_foo_bar():
    bar = make_foo_bar_sync(label="hi", count=2)
    assert round_trip_foo_bar_sync(b=bar) == bar


def test_symbol_collisions_round_trip_fizz_foo_bar():
    bar = make_fizz_foo_bar_sync(tag="t", ratio=1.5)
    assert round_trip_fizz_foo_bar_sync(b=bar) == bar


def test_symbol_collisions_round_trip_fizz_buzz_foo_bar():
    bar = make_fizz_buzz_foo_bar_sync(flavor="f", weight=2.5, active=True)
    assert round_trip_fizz_buzz_foo_bar_sync(b=bar) == bar


def test_symbol_collisions_round_trip_ipsum():
    ipsum = make_ipsum_sync(
        bar1=make_foo_bar_sync(label="a", count=1),
        bar2=make_fizz_foo_bar_sync(tag="b", ratio=2.0),
        bar3=make_fizz_buzz_foo_bar_sync(flavor="c", weight=3.0, active=False),
    )
    assert round_trip_ipsum_sync(i=ipsum) == ipsum


def test_symbol_collisions_round_trip_deep():
    ipsum = make_ipsum_sync(
        bar1=make_foo_bar_sync(label="a", count=1),
        bar2=make_fizz_foo_bar_sync(tag="b", ratio=2.0),
        bar3=make_fizz_buzz_foo_bar_sync(flavor="c", weight=3.0, active=False),
    )
    deep = make_deep_sync(
        here=make_foo_bar_sync(label="h", count=9),
        there=make_fizz_foo_bar_sync(tag="th", ratio=4.0),
        further=make_fizz_buzz_foo_bar_sync(flavor="fu", weight=5.0, active=True),
        nested=ipsum,
    )
    assert round_trip_deep_sync(d=deep) == deep
