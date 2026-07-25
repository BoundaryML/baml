"""Roundtrip coverage for `baml_sdk.recursion` — recursive classes / SCCs.

All recursive child fields are optional, so finite values are built by
terminating recursion with `None`.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.recursion import (
    IntBinaryTree,
    A,
    B,
    T1,
    T2,
    T3,
    T4,
    T5,
    T6,
    round_trip_int_binary_tree,
    round_trip_a,
    round_trip_b,
    round_trip_t1,
    round_trip_t2,
    round_trip_t3,
    round_trip_t4,
    round_trip_t5,
    round_trip_t6,
)


def test_recursion_round_trip_int_binary_tree():
    t = IntBinaryTree(
        value=1,
        left=IntBinaryTree(value=2, left=None, right=None),
        right=None,
    )
    assert round_trip_int_binary_tree(t=t) == t


def test_recursion_round_trip_mutual_recursion():
    a = A(b=B(a=None))
    b = B(a=A(b=None))
    assert round_trip_a(a=a) == a
    assert round_trip_b(b=b) == b


def test_recursion_round_trip_scc_t1_t2_t3():
    t1 = T1(via2=T2(via1=None, via3=None), via3=None)
    t2 = T2(via1=None, via3=T3(via1=None, via2=None))
    t3 = T3(via1=None, via2=None)
    assert round_trip_t1(t=t1) == t1
    assert round_trip_t2(t=t2) == t2
    assert round_trip_t3(t=t3) == t3


def test_recursion_round_trip_scc_t4_t5_t6():
    t4 = T4(via5=T5(via4=None, via6=None), via6=None)
    t5 = T5(via4=None, via6=T6(via4=None, via5=None))
    t6 = T6(via4=None, via5=None)
    assert round_trip_t4(t=t4) == t4
    assert round_trip_t5(t=t5) == t5
    assert round_trip_t6(t=t6) == t6
