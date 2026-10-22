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
