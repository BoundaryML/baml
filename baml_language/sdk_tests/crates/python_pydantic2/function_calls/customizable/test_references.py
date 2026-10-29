"""Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
references: one passed back is the same instance, wherever a class can sit.
Its state shows which instance a call reached: one hasher fed "a" and then
"b" hashes as a fresh one fed "ab"."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.baml.hash import DefaultHasher
from baml_sdk.references import (
    HasherMarker,
    describe_hasher_or_marker,
    hash_after,
    hasher_or_marker,
    hasher_through_callback,
    hasher_twice,
    hasher_under,
    hold_hasher,
    make_hasher,
    maybe_hasher,
    same_hasher,
)


def test_references_a_returned_instance_passed_back_is_the_same_instance():
    hasher = make_hasher()
    hash_after(hasher, "a")
    fed_twice = hash_after(hasher, "b")
    assert fed_twice == hash_after(make_hasher(), "ab")
    # Negative control: a fresh hasher fed only "b" differs.
    assert fed_twice != hash_after(make_hasher(), "b")


def test_references_one_engine_instance_is_one_reference():
    hasher = make_hasher()
    assert isinstance(hasher, DefaultHasher)
    assert same_hasher(hasher) == hasher
    assert make_hasher() != hasher


def test_references_containers_hold_the_instance():
    hasher = make_hasher()
    assert isinstance(maybe_hasher(True), DefaultHasher)
    assert maybe_hasher(False) is None
    assert hasher_twice(hasher) == [hasher, hasher]
    assert hasher_under("key", hasher) == {"key": hasher}


def test_references_a_field_holds_the_instance():
    hasher = make_hasher()
    holder = hold_hasher("label", hasher)
    assert holder.label == "label"
    assert holder.hasher == hasher


def test_references_a_union_selects_the_instance_arm():
    assert isinstance(hasher_or_marker(True), DefaultHasher)
    assert hasher_or_marker(False) == HasherMarker(tag="marker")
    assert describe_hasher_or_marker(make_hasher()) == "hasher"
    assert describe_hasher_or_marker(HasherMarker(tag="marker")) == "marker"


def test_references_a_callback_receives_and_returns_the_instance():
    hasher = make_hasher()
    received = []

    def callback(value):
        received.append(value)
        return value

    assert hasher_through_callback(hasher, callback) == hasher
    assert received == [hasher]
