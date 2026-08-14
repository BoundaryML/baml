import pytest

import baml_sdk  # noqa: F401
from baml_sdk import OptBox, baml, optional_args_probe, optional_args_probe_async


def test_optional_args_runtime_matrix():
    assert optional_args_probe(1) == [1, 5, 99]
    assert optional_args_probe(1, opt1=baml.UNSET, opt2=baml.UNSET) == [1, 5, 99]
    assert optional_args_probe(arg0=1) == [1, 5, 99]
    assert optional_args_probe(1, opt1=None) == [1, None, 99]
    assert optional_args_probe(1, opt1=8) == [1, 8, 99]
    assert optional_args_probe(1, opt2=None) == [1, 5, None]
    assert optional_args_probe(1, opt2=9) == [1, 5, 9]
    assert optional_args_probe(1, opt1=None, opt2=None) == [1, None, None]
    assert optional_args_probe(1, opt1=8, opt2=9) == [1, 8, 9]


def test_optional_args_python_unset_and_none_differ_in_one_call():
    # UNSET means "omit this argument"; None means "pass an explicit null".
    # The two must stay distinct within a single call.
    assert baml.UNSET is not None
    assert optional_args_probe(1, opt1=baml.UNSET, opt2=None) == [1, 5, None]
    assert optional_args_probe(1, opt1=None, opt2=baml.UNSET) == [1, None, 99]


async def test_optional_args_async_samples():
    assert await optional_args_probe_async(1) == [1, 5, 99]
    assert await optional_args_probe_async(1, opt1=None) == [1, None, 99]
    assert await optional_args_probe_async(1, opt2=9) == [1, 5, 9]


def test_optional_args_opt_box_method_matrix():
    box = OptBox.make(10)
    assert box.base == 17

    box2 = OptBox.make(10, opt1=0)
    assert box2.base == 10
    assert box2.probe(1) == [10, 1, 5]
    assert box2.probe(1, opt1=8) == [10, 1, 8]


def test_optional_args_negative_runtime_cases_reject():
    with pytest.raises(Exception):
        optional_args_probe(1, **{"opt3": 1})
    with pytest.raises(Exception):
        optional_args_probe(**{})
    with pytest.raises(TypeError):
        optional_args_probe(1, arg0=1)
