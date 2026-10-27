"""The walker reads a container's items one at a time under one budget per
call: a list whose `__iter__` never ends is refused at the budget, and a
`__len__` that lies sizes nothing."""

import pytest

from baml_bridge.baml_py import _encode_inbound


class Endless(list):
    """A list whose iteration never ends."""

    def __iter__(self):
        while True:
            yield 1


class LyingLength(list):
    """A list that claims a billion items and yields three."""

    def __len__(self):
        return 1_000_000_000

    def __iter__(self):
        yield from (1, 2, 3)


def test_an_endless_iterator_is_refused_at_the_node_budget():
    with pytest.raises(TypeError, match="more than 1048576 nodes"):
        _encode_inbound(Endless(), "values")


def test_a_lying_length_sizes_nothing():
    # Three items read, three encoded: the length was never consulted.
    assert _encode_inbound(LyingLength(), "values")


class MutatesItsOwner(list):
    """A list whose iteration adds a key to the dict that holds it."""

    owner: dict

    def __iter__(self):
        self.owner["late"] = 1
        yield 1


def test_a_dict_mutated_while_it_is_read_is_the_hosts_error():
    # Reading a value runs host code that may mutate the dict being read.
    # Python answers that with its own `RuntimeError`, which the walker
    # reports as the host's error; it is never a bridge panic.
    owner: dict = {}
    item = MutatesItsOwner()
    item.owner = owner
    owner["a"] = item
    with pytest.raises(RuntimeError, match="changed size"):
        _encode_inbound(owner, "values")
