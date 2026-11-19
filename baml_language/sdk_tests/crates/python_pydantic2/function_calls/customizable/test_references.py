"""Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
references: one passed back is the same instance, wherever a class can sit.
Its state shows which instance a call reached: one hasher fed "a" and then
"b" hashes as a fresh one fed "ab"."""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.baml.hash import DefaultHasher
from baml_sdk.references import (
    HasherMarker,
    describe_hasher_or_marker_sync,
    hash_after_sync,
    hasher_or_marker_sync,
    hasher_through_callback_sync,
    hasher_twice_sync,
    hasher_under_sync,
    hold_hasher_sync,
    make_hasher_sync,
    maybe_hasher_sync,
    same_hasher_sync,
)


def test_references_a_returned_instance_passed_back_is_the_same_instance():
    hasher = make_hasher_sync()
    hash_after_sync(hasher, "a")
    fed_twice = hash_after_sync(hasher, "b")
    assert fed_twice == hash_after_sync(make_hasher_sync(), "ab")
    # Negative control: a fresh hasher fed only "b" differs.
    assert fed_twice != hash_after_sync(make_hasher_sync(), "b")


def test_references_one_engine_instance_is_one_reference():
    hasher = make_hasher_sync()
    assert isinstance(hasher, DefaultHasher)
    assert same_hasher_sync(hasher) == hasher
    assert make_hasher_sync() != hasher


def test_references_containers_hold_the_instance():
    hasher = make_hasher_sync()
    assert isinstance(maybe_hasher_sync(True), DefaultHasher)
    assert maybe_hasher_sync(False) is None
    assert hasher_twice_sync(hasher) == [hasher, hasher]
    assert hasher_under_sync("key", hasher) == {"key": hasher}


def test_references_a_field_holds_the_instance():
    hasher = make_hasher_sync()
    holder = hold_hasher_sync("label", hasher)
    assert holder.label == "label"
    assert holder.hasher == hasher


def test_references_a_union_selects_the_instance_arm():
    assert isinstance(hasher_or_marker_sync(True), DefaultHasher)
    assert hasher_or_marker_sync(False) == HasherMarker(tag="marker")
    assert describe_hasher_or_marker_sync(make_hasher_sync()) == "hasher"
    assert describe_hasher_or_marker_sync(HasherMarker(tag="marker")) == "marker"


def test_references_a_callback_receives_and_returns_the_instance():
    hasher = make_hasher_sync()
    received = []

    def callback(value):
        received.append(value)
        return value

    assert hasher_through_callback_sync(hasher, callback) == hasher
    assert received == [hasher]
