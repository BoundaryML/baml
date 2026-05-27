"""Roundtrip coverage for `baml_sdk.media`.

Media values can't be hand-built as plain dicts, so each value is sourced
from the matching `return_*` function (which builds it engine-side via
`image.from_url(...)` etc.). The *decode* path works — `return_*` yields a
`BamlImage`/`BamlAudio`/… PyO3 object. The *encode* path is broken (see
35b, "Bug B: media values can't be re-encoded as arguments"): passing one
of those values back into a function fails with

    BamlClientError: Type mismatch: Unknown class `` in external Instance value

because the typemap reverse-map seed is keyed on
`("baml_core.baml_py", "BamlImage")` while the PyO3 type's actual
`__module__` is `"builtins"`, so `py_type_to_baml_type` returns "". The
`round_trip_*` cases are `xfail`-marked until that's fixed.
"""

import pytest

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.media import (
    Media,
    return_image,
    return_audio,
    return_video,
    return_pdf,
    round_trip_image,
    round_trip_audio,
    round_trip_video,
    round_trip_pdf,
    round_trip_media,
)

URL = "https://example.com/asset"

_ENCODE_BUG = (
    "Bug B (35b): media value re-encode fails — typemap reverse seed is "
    "keyed ('baml_core.baml_py', 'BamlImage') but the PyO3 type's "
    "__module__ is 'builtins', so py_type_to_baml_type returns ''."
)


# --- decode path (return_*) works -----------------------------------------


def test_return_image():
    assert return_image(url=URL, mime=None) is not None


def test_return_audio():
    assert return_audio(url=URL, mime=None) is not None


def test_return_video():
    assert return_video(url=URL, mime=None) is not None


def test_return_pdf():
    assert return_pdf(url=URL, mime=None) is not None


# --- encode path (round_trip_*) is broken: Bug B --------------------------


@pytest.mark.xfail(reason=_ENCODE_BUG, strict=True)
def test_round_trip_image():
    img = return_image(url=URL, mime=None)
    assert round_trip_image(x=img) is not None


@pytest.mark.xfail(reason=_ENCODE_BUG, strict=True)
def test_round_trip_audio():
    aud = return_audio(url=URL, mime=None)
    assert round_trip_audio(x=aud) is not None


@pytest.mark.xfail(reason=_ENCODE_BUG, strict=True)
def test_round_trip_video():
    vid = return_video(url=URL, mime=None)
    assert round_trip_video(x=vid) is not None


@pytest.mark.xfail(reason=_ENCODE_BUG, strict=True)
def test_round_trip_pdf():
    pdf = return_pdf(url=URL, mime=None)
    assert round_trip_pdf(x=pdf) is not None


@pytest.mark.xfail(reason=_ENCODE_BUG, strict=True)
def test_round_trip_media():
    m = Media(
        image_field=return_image(url=URL, mime=None),
        audio_field=return_audio(url=URL, mime=None),
        video_field=return_video(url=URL, mime=None),
        pdf_field=return_pdf(url=URL, mime=None),
    )
    assert round_trip_media(m=m) is not None
