"""Roundtrip coverage for `baml_sdk.generics` — generic classes (over `<int>`).

The generic *instance method* path (`WrapperMethods.get_value` /
`get_value_or_marker`) is covered separately in
`customizable/test_generic.py`; here we cover the
concretely-instantiated generic class round trips.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.generics import (
    Wrapper,
    GenericLinkedList,
    GenericBinaryTree,
    Box,
    NestedGenerics,
    DifferingInstantiation,
    round_trip_wrapper_int,
    round_trip_generic_linked_list_int,
    round_trip_generic_binary_tree_int,
    round_trip_box_int,
    round_trip_nested_generics,
    round_trip_differing_instantiation,
)


def test_generics_round_trip_wrapper_int():
    w = Wrapper[int](value=5)
    assert round_trip_wrapper_int(w=w) == w


def test_generics_round_trip_generic_linked_list_int():
    ll = GenericLinkedList[int](value=1, next=GenericLinkedList[int](value=2, next=None))
    assert round_trip_generic_linked_list_int(l=ll) == ll


def test_generics_round_trip_generic_binary_tree_int():
    t = GenericBinaryTree[int](value=1, left=None, right=None)
    assert round_trip_generic_binary_tree_int(t=t) == t


def test_generics_round_trip_box_int():
    b = Box[int](value=3, wrapped=Wrapper[int](value=4))
    assert round_trip_box_int(b=b) == b


def test_generics_round_trip_nested_generics():
    n = NestedGenerics(
        ww=Wrapper[Wrapper[int]](value=Wrapper[int](value=1)),
        wl=Wrapper[list](value=[1, 2]),
        wr=Wrapper[GenericLinkedList[int]](
            value=GenericLinkedList[int](value=9, next=None)
        ),
    )
    assert round_trip_nested_generics(n=n) == n


def test_generics_round_trip_differing_instantiation():
    d = DifferingInstantiation(
        list=GenericLinkedList[Wrapper[int]](value=Wrapper[int](value=1), next=None)
    )
    assert round_trip_differing_instantiation(d=d) == d
