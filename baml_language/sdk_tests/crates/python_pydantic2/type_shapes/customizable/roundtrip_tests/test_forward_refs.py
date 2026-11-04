"""Roundtrip coverage for `baml_sdk.forward_refs` — forward references.

`round_trip_node` is intentionally NOT exercised: `class Node { next Node }`
has a *required* (non-optional) self-reference, so no finite value can be
constructed from the host side. It still emits and type-checks; the
import below proves the symbol exists.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.forward_refs import (
    Other,
    GNode,
    round_trip_other_sync,
    round_trip_rec_list_sync,
    round_trip_rec_list_with_other_sync,
    round_trip_node_sync,  # noqa: F401 — uninhabitable (required self-ref); import-only
    round_trip_g_node_int_sync,
)


def test_forward_refs_round_trip_other():
    o = Other(v=7)
    assert round_trip_other_sync(o=o) == o


def test_forward_refs_round_trip_rec_list():
    assert round_trip_rec_list_sync(r=[1, [2, 3]]) == [1, [2, 3]]


def test_forward_refs_round_trip_rec_list_with_other():
    # RecListWithOther = int | Other | RecListWithOther[]
    assert round_trip_rec_list_with_other_sync(r=1) == 1
    assert round_trip_rec_list_with_other_sync(r=[1, 2]) == [1, 2]


def test_forward_refs_round_trip_g_node_int():
    # The leaf node carries `children=[]`; this exercises the empty-list
    # round trip fixed under Bug A (35b).
    g = GNode[int](children=[GNode[int](children=[])])
    assert round_trip_g_node_int_sync(g=g) == g
