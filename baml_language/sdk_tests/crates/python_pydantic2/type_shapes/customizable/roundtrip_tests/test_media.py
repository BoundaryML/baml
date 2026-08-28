"""Roundtrip coverage for `baml_sdk.media`.

Media values can't be hand-built as plain dicts, so each value is sourced
from the matching `return_*` function (which builds it engine-side via
`image.from_url(...)` etc.). The *decode* path yields a
`BamlImage`/`BamlAudio`/… PyO3 object; the *encode* path passes that value
back into a `round_trip_*` function.

Both directions work since 35c: the media PyO3 types are declared
`#[pyclass(module = "baml_bridge.baml_py")]`, so `type(value).__module__`
matches the typemap reverse-map seed `("baml_bridge.baml_py", "BamlImage")`
and `py_type_to_baml_type` resolves the engine FQN on encode. (Before
that fix PyO3 reported `__module__ == "builtins"`, the seed missed, and
re-encode failed with `Unknown class ``— 35b "Bug B".)
"""

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
    round_trip_optional_image_list,
    image_mime_type,
)
from baml_sdk.baml.media import Image

URL = "https://example.com/asset"
PNG_B64 = "iVBORw0KGgo="


# --- decode path (return_*) works -----------------------------------------


def test_media_return_image():
    assert return_image(url=URL, mime=None) is not None


def test_media_return_audio():
    assert return_audio(url=URL, mime=None) is not None


def test_media_return_video():
    assert return_video(url=URL, mime=None) is not None


def test_media_return_pdf():
    assert return_pdf(url=URL, mime=None) is not None


# --- encode path (round_trip_*) ------------------------------------------


def test_media_round_trip_image():
    img = return_image(url=URL, mime=None)
    assert round_trip_image(x=img) is not None


def test_media_round_trip_audio():
    aud = return_audio(url=URL, mime=None)
    assert round_trip_audio(x=aud) is not None


def test_media_round_trip_video():
    vid = return_video(url=URL, mime=None)
    assert round_trip_video(x=vid) is not None


def test_media_round_trip_pdf():
    pdf = return_pdf(url=URL, mime=None)
    assert round_trip_pdf(x=pdf) is not None


def test_media_round_trip_media():
    m = Media(
        image_field=return_image(url=URL, mime=None),
        audio_field=return_audio(url=URL, mime=None),
        video_field=return_video(url=URL, mime=None),
        pdf_field=return_pdf(url=URL, mime=None),
    )
    assert round_trip_media(m=m) is not None


def test_host_created_media_round_trips_through_optional_list():
    image = Image.from_base64(PNG_B64, "image/png")

    # The engine must reconstruct the stdlib Image wrapper, not expose its
    # private rust-data payload directly, before BAML dispatches this method.
    assert image_mime_type(x=image) == "image/png"

    nonempty = round_trip_optional_image_list(x=[image])
    assert nonempty is not None
    assert len(nonempty) == 1
    assert nonempty[0].base64() == PNG_B64
    assert nonempty[0].mime_type() == "image/png"

    assert round_trip_optional_image_list(x=[]) == []
    assert round_trip_optional_image_list(x=None) is None
