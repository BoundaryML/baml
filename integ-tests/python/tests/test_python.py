"""Test the compatibility of baml_py with the Python ecosystem."""

import baml_py
import inspect
import pickle
import pydantic
import pytest


def test_inspect():
    """Assert that baml_py is compatible with the inspect module.

    This is a regression test for a bug where `inspect.stack()` would implode if the
    pyo3 code called `PyModule::from_code` without specifying the `file_name` arg (i.e.
    without specifying the source file metadata for the inline Python snippet).
    """

    class LoremIpsum(pydantic.BaseModel):  # pyright: ignore[reportUnusedClass]
        """Defining this Pydantic model alone is sufficient to trigger the bug."""

        my_image: baml_py.Image
        my_audio: baml_py.Audio

    try:
        inspect.stack()
    except Exception as e:
        pytest.fail(f"inspect.stack() raised an unexpected exception: {e}")


def test_pickle():
    i = baml_py.Image.from_url("https://example.com/image.png")
    p = pickle.dumps(i)
    assert i == pickle.loads(pickle.dumps(i))
    assert p == pickle.dumps(pickle.loads(p))

    i2 = baml_py.Image.from_url("https://example.com/image.jpg")
    p2 = pickle.dumps(i2)
    assert i2 == pickle.loads(pickle.dumps(i2))
    assert p2 == pickle.dumps(pickle.loads(p2))

    i3 = baml_py.Image.from_base64("image/png", "iVBORw0KGgoAAAANSUhEUgAAAAUA")
    p3 = pickle.dumps(i3)
    assert i3 == pickle.loads(pickle.dumps(i3))
    assert p3 == pickle.dumps(pickle.loads(p3))

"""
from multiprocessing import Process
from baml_client import b


def run():
    b.ExtractResume("test")


if __name__ == "__main__":
    current = Process(target=run)
    current.start()
    current.join()
"""
# from baml_client import b
# def run():
#     b.ExtractResume("test")

# def test_pickle_multiprocessing():
#     from multiprocessing import Process

#     current = Process(target=run)
#     current.start()
#     current.join()

def test_baml_client_pickle_roundtrip():
    import pickle
    from baml_client import b
    # Pickle and unpickle the b object
    pickled = pickle.dumps(b)
    b2 = pickle.loads(pickled)
    # Check type and that no error occurs
    assert type(b2) is type(b)